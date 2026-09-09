pub trait StorageNamespace {
    fn user_sub(&self) -> &str;

    fn application_id(&self) -> i64;
}
