use std::sync::Arc;

use chrono::{DateTime, Timelike, Utc};
use db::{
    Engine, FromRow, FromRowError, Kernel, Query, QueryColumn, QueryExpr, QueryExprValue,
    QueryFrom, QueryInsert, QuerySelect, QueryUpdate, QueryUpdateAssignment, Row, RowCodec,
    Statement, Uuid, Value,
};
use idp_model::{
    contract::{DeviceState, TrustedDevice},
    model::{Device, Id},
    replica::allows_tunnel_access,
};

use crate::{DeviceRepo, ManagementError, ManagementResult};

const TABLE: &str = "devices";
const COLUMNS: [&str; 12] = [
    "id",
    "name",
    "public_key",
    "address",
    "enrollment_code_hash",
    "enrollment_expires_at",
    "pairing_accepting_public_key",
    "state",
    "approved_at",
    "revoked_at",
    "created_at",
    "updated_at",
];

pub struct DbDeviceRepo<K, R>
where
    K: Kernel,
    R: RowCodec<K::Transaction>,
{
    engine: Arc<Engine<K, R>>,
}

impl<K, R> DbDeviceRepo<K, R>
where
    K: Kernel,
    R: RowCodec<K::Transaction>,
{
    #[must_use]
    pub const fn new(engine: Arc<Engine<K, R>>) -> Self {
        Self { engine }
    }

    async fn records(&self) -> ManagementResult<Vec<DeviceRow>> {
        let mut results = self
            .engine
            .execute(vec![Statement::Query(select())])
            .await
            .map_err(db_error)?;
        results
            .pop()
            .ok_or_else(|| ManagementError::InvalidInput("missing device query result".into()))?
            .rows_as::<DeviceRow>()
            .map_err(row_error)
    }

    async fn record(&self, id: Id) -> ManagementResult<Option<DeviceRow>> {
        Ok(self.records().await?.into_iter().find(|row| row.id == id))
    }

    async fn ensure_clear(&self, id: Id) -> ManagementResult<()> {
        if allows_tunnel_access(&self.engine, &[id])
            .await
            .map_err(db_error)?
        {
            Ok(())
        } else {
            Err(ManagementError::InvalidInput("conflicted device".into()))
        }
    }

    async fn update(
        &self,
        id: Id,
        assignments: Vec<QueryUpdateAssignment>,
    ) -> ManagementResult<()> {
        self.engine
            .execute(vec![Statement::Query(Query::Update(QueryUpdate {
                from: from(),
                assignments,
                predicate: Some(equals("id", Value::Uuid(id))),
                returning: None,
            }))])
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

impl<K, R> DeviceRepo for DbDeviceRepo<K, R>
where
    K: Kernel,
    R: RowCodec<K::Transaction>,
{
    async fn create(
        &self,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> ManagementResult<Device> {
        let now = now();
        let state = if self.records().await?.is_empty() {
            DeviceState::Approved
        } else {
            DeviceState::Pending
        };
        let row = DeviceRow {
            id: Id::now_v7(),
            name,
            public_key,
            address,
            enrollment_code_hash: (state == DeviceState::Pending).then_some(enrollment_code_hash),
            enrollment_expires_at: (state == DeviceState::Pending).then_some(enrollment_expires_at),
            pairing_accepting_public_key: None,
            state,
            approved_at: None,
            revoked_at: None,
            created_at: now.timestamp(),
            updated_at: now.timestamp(),
        };
        self.engine
            .execute(vec![Statement::Query(Query::Insert(QueryInsert {
                table: TABLE.into(),
                row: row.clone().into(),
                returning: None,
            }))])
            .await
            .map_err(db_error)?;
        row.try_into()
    }

    async fn create_pairing(
        &self,
        name: String,
        public_key: String,
        address: String,
        accepting_public_key: String,
    ) -> ManagementResult<Device> {
        let now = now();
        let row = DeviceRow {
            id: Id::now_v7(),
            name,
            public_key,
            address,
            enrollment_code_hash: None,
            enrollment_expires_at: None,
            pairing_accepting_public_key: Some(accepting_public_key),
            state: DeviceState::Pending,
            approved_at: None,
            revoked_at: None,
            created_at: now.timestamp(),
            updated_at: now.timestamp(),
        };
        self.engine
            .execute(vec![Statement::Query(Query::Insert(QueryInsert {
                table: TABLE.into(),
                row: row.clone().into(),
                returning: None,
            }))])
            .await
            .map_err(db_error)?;
        row.try_into()
    }

    async fn pending_pairing(&self, device_id: Id) -> ManagementResult<Option<(Device, String)>> {
        let Some(row) = self.record(device_id).await? else {
            return Ok(None);
        };
        self.ensure_clear(device_id).await?;
        if row.state != DeviceState::Pending {
            return Ok(None);
        }
        let Some(key) = row.pairing_accepting_public_key.clone() else {
            return Ok(None);
        };
        Ok(Some((row.try_into()?, key)))
    }

    async fn approve_pairing(&self, device_id: Id) -> ManagementResult<Option<Device>> {
        let Some(row) = self.record(device_id).await? else {
            return Ok(None);
        };
        self.ensure_clear(device_id).await?;
        if row.state != DeviceState::Pending || row.pairing_accepting_public_key.is_none() {
            return Ok(None);
        }
        let updated_at = now();
        self.update(
            device_id,
            vec![
                assignment("state", Value::Integer(DeviceState::Approved as i64)),
                assignment("pairing_accepting_public_key", Value::Null),
                assignment("updated_at", Value::Integer(updated_at.timestamp())),
            ],
        )
        .await?;
        DeviceRow {
            state: DeviceState::Approved,
            pairing_accepting_public_key: None,
            updated_at: updated_at.timestamp(),
            ..row
        }
        .try_into()
        .map(Some)
    }

    async fn has_any(&self) -> ManagementResult<bool> {
        Ok(!self.records().await?.is_empty())
    }

    async fn list(&self) -> ManagementResult<Vec<Device>> {
        let rows = self.records().await?;
        for row in &rows {
            self.ensure_clear(row.id).await?;
        }
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_approved(&self) -> ManagementResult<Vec<TrustedDevice>> {
        let rows = self.records().await?;
        let mut devices = Vec::new();
        for row in rows
            .into_iter()
            .filter(|row| row.state == DeviceState::Approved)
        {
            self.ensure_clear(row.id).await?;
            devices.push(TrustedDevice {
                public_key: row.public_key,
                address: row.address,
            });
        }
        Ok(devices)
    }

    async fn are_approved(
        &self,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> ManagementResult<bool> {
        let rows = self.records().await?;
        let mut local = false;
        let mut remote = false;
        for row in rows
            .into_iter()
            .filter(|row| row.state == DeviceState::Approved)
        {
            self.ensure_clear(row.id).await?;
            local |= row.public_key == local_public_key;
            remote |= row.public_key == remote_public_key;
        }
        Ok(local && remote)
    }

    async fn approve(
        &self,
        device_id: Id,
        enrollment_code_hash: &[u8],
    ) -> ManagementResult<Option<Device>> {
        let Some(row) = self.record(device_id).await? else {
            return Ok(None);
        };
        self.ensure_clear(device_id).await?;
        if row.state != DeviceState::Pending
            || row.enrollment_code_hash.as_deref() != Some(enrollment_code_hash)
            || row
                .enrollment_expires_at
                .is_none_or(|expires_at| expires_at <= now().timestamp())
        {
            return Ok(None);
        }
        let updated_at = now();
        self.update(
            device_id,
            vec![
                assignment("state", Value::Integer(DeviceState::Approved as i64)),
                assignment("enrollment_code_hash", Value::Null),
                assignment("enrollment_expires_at", Value::Null),
                assignment("updated_at", Value::Integer(updated_at.timestamp())),
            ],
        )
        .await?;
        DeviceRow {
            state: DeviceState::Approved,
            enrollment_code_hash: None,
            enrollment_expires_at: None,
            updated_at: updated_at.timestamp(),
            ..row
        }
        .try_into()
        .map(Some)
    }

    async fn rename(&self, device_id: Id, name: String) -> ManagementResult<Option<Device>> {
        let Some(row) = self.record(device_id).await? else {
            return Ok(None);
        };
        self.ensure_clear(device_id).await?;
        if row.state == DeviceState::Revoked {
            return Ok(None);
        }
        let updated_at = now();
        self.update(
            device_id,
            vec![
                assignment("name", Value::Text(name.clone())),
                assignment("updated_at", Value::Integer(updated_at.timestamp())),
            ],
        )
        .await?;
        DeviceRow {
            name,
            updated_at: updated_at.timestamp(),
            ..row
        }
        .try_into()
        .map(Some)
    }

    async fn revoke(&self, device_id: Id, protected_public_key: &str) -> ManagementResult<bool> {
        let Some(row) = self.record(device_id).await? else {
            return Ok(false);
        };
        self.ensure_clear(device_id).await?;
        if row.public_key == protected_public_key || row.state == DeviceState::Revoked {
            return Ok(false);
        }
        let rows = self.records().await?;
        for other in rows
            .iter()
            .filter(|other| other.state == DeviceState::Approved)
        {
            self.ensure_clear(other.id).await?;
        }
        if row.state == DeviceState::Approved
            && rows
                .iter()
                .filter(|other| other.state == DeviceState::Approved)
                .count()
                <= 1
        {
            return Ok(false);
        }
        let updated_at = now();
        self.update(
            device_id,
            vec![
                assignment("state", Value::Integer(DeviceState::Revoked as i64)),
                assignment("revoked_at", Value::Integer(updated_at.timestamp())),
                assignment("updated_at", Value::Integer(updated_at.timestamp())),
            ],
        )
        .await?;
        Ok(true)
    }

    async fn revoke_self(&self, public_key: &str) -> ManagementResult<bool> {
        let rows = self.records().await?;
        let mut found = false;
        for row in rows.into_iter().filter(|row| row.public_key == public_key) {
            found = true;
            self.ensure_clear(row.id).await?;
            if row.state == DeviceState::Revoked {
                continue;
            }
            let updated_at = now();
            self.update(
                row.id,
                vec![
                    assignment("state", Value::Integer(DeviceState::Revoked as i64)),
                    assignment("revoked_at", Value::Integer(updated_at.timestamp())),
                    assignment("updated_at", Value::Integer(updated_at.timestamp())),
                ],
            )
            .await?;
        }
        Ok(found)
    }
}

#[derive(Clone, Debug)]
struct DeviceRow {
    id: Uuid,
    name: String,
    public_key: String,
    address: String,
    enrollment_code_hash: Option<Vec<u8>>,
    enrollment_expires_at: Option<i64>,
    pairing_accepting_public_key: Option<String>,
    state: DeviceState,
    approved_at: Option<i64>,
    revoked_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl FromRow for DeviceRow {
    fn from_row(row: &Row, columns: &[&str]) -> Result<Self, FromRowError> {
        let state = db::decode::<i64>(db::value(row, columns, "state")?, "state")?;
        let state = match state {
            0 => DeviceState::Pending,
            1 => DeviceState::Approved,
            2 => DeviceState::Revoked,
            _ => {
                return Err(FromRowError::InvalidValue {
                    column: "state".into(),
                    expected: db::ValueType::Integer,
                    actual: db::ValueType::Integer,
                });
            }
        };
        Ok(Self {
            id: db::decode(db::value(row, columns, "id")?, "id")?,
            name: db::decode(db::value(row, columns, "name")?, "name")?,
            public_key: db::decode(db::value(row, columns, "public_key")?, "public_key")?,
            address: db::decode(db::value(row, columns, "address")?, "address")?,
            enrollment_code_hash: db::decode(
                db::value(row, columns, "enrollment_code_hash")?,
                "enrollment_code_hash",
            )?,
            enrollment_expires_at: db::decode(
                db::value(row, columns, "enrollment_expires_at")?,
                "enrollment_expires_at",
            )?,
            pairing_accepting_public_key: db::decode(
                db::value(row, columns, "pairing_accepting_public_key")?,
                "pairing_accepting_public_key",
            )?,
            state,
            approved_at: db::decode(db::value(row, columns, "approved_at")?, "approved_at")?,
            revoked_at: db::decode(db::value(row, columns, "revoked_at")?, "revoked_at")?,
            created_at: db::decode(db::value(row, columns, "created_at")?, "created_at")?,
            updated_at: db::decode(db::value(row, columns, "updated_at")?, "updated_at")?,
        })
    }
}

impl TryFrom<DeviceRow> for Device {
    type Error = ManagementError;

    fn try_from(row: DeviceRow) -> ManagementResult<Self> {
        Ok(Self {
            id: row.id,
            name: row.name,
            public_key: row.public_key,
            address: row.address,
            state: row.state,
            created_at: timestamp(row.created_at)?,
            updated_at: timestamp(row.updated_at)?,
            revoked_at: row.revoked_at.map(timestamp).transpose()?,
        })
    }
}

impl From<DeviceRow> for Row {
    fn from(row: DeviceRow) -> Self {
        Row::new(vec![
            Value::Uuid(row.id),
            Value::Text(row.name),
            Value::Text(row.public_key),
            Value::Text(row.address),
            row.enrollment_code_hash.map_or(Value::Null, Value::Blob),
            row.enrollment_expires_at
                .map_or(Value::Null, Value::Integer),
            row.pairing_accepting_public_key
                .map_or(Value::Null, Value::Text),
            Value::Integer(row.state as i64),
            row.approved_at.map_or(Value::Null, Value::Integer),
            row.revoked_at.map_or(Value::Null, Value::Integer),
            Value::Integer(row.created_at),
            Value::Integer(row.updated_at),
        ])
    }
}

fn now() -> DateTime<Utc> {
    Utc::now()
        .with_nanosecond(0)
        .expect("zero nanoseconds is valid")
}

fn timestamp(value: i64) -> ManagementResult<DateTime<Utc>> {
    DateTime::from_timestamp(value, 0)
        .ok_or_else(|| ManagementError::InvalidInput("invalid device timestamp".into()))
}

fn db_error(error: db::EngineError) -> ManagementError {
    ManagementError::InvalidInput(error.to_string())
}

fn row_error(error: FromRowError) -> ManagementError {
    ManagementError::InvalidInput(error.to_string())
}

fn from() -> QueryFrom {
    QueryFrom {
        table: TABLE.into(),
        joins: vec![],
    }
}

fn column(name: &str) -> QueryColumn {
    QueryColumn::new(TABLE.into(), name.into())
}

fn equals(name: &str, value: Value) -> QueryExpr {
    QueryExpr::Equals(
        Box::new(QueryExpr::Value(QueryExprValue::Column(column(name)))),
        Box::new(QueryExpr::Value(QueryExprValue::Value(value))),
    )
}

fn assignment(name: &str, value: Value) -> QueryUpdateAssignment {
    QueryUpdateAssignment {
        column: column(name),
        value: QueryExprValue::Value(value),
    }
}

fn select() -> Query {
    Query::Select(QuerySelect {
        from: from(),
        projection: COLUMNS.into_iter().map(column).collect(),
        predicate: None,
        aggregates: vec![],
        group_by: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
        having: None,
    })
}
