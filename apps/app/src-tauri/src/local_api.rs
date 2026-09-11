use axum::{Router, response::Html, routing::get};

const TRUST_PAGE: &str = "<!DOCTYPE html>\
<html lang=\"en\">\
<head>\
<meta charset=\"utf-8\">\
<title>Localhost Certificate</title>\
</head>\
<body>\
<h1>Localhost certificate</h1>\
<p>If your browser shows a certificate warning, accept it to allow local apps to connect.</p>\
<p>You can close this tab after the page loads without warnings.</p>\
</body>\
</html>";

pub fn router() -> Router {
    Router::new().route("/trust", get(trust))
}

async fn trust() -> Html<&'static str> {
    Html(TRUST_PAGE)
}
