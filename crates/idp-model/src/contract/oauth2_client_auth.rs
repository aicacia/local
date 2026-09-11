#[derive(Clone, Debug)]
pub struct OAuth2ClientAuth {
    pub client_id: String,
    pub client_secret: Option<String>,
}
