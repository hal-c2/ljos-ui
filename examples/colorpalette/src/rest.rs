//! The typed REST core the three QML types stand on.
//!
//! Everything here is plain values: a request is built, a reply is
//! decoded, a state machine moves. Nothing here knows how a request
//! travels — that is the platform's business. [`Request::to_platform`]
//! is where a core request becomes the platform's request value
//! (method, URL, a JSON body with its content type), which
//! `Platform::request` carries for every method this client builds;
//! replies come back the same way, and [`Session::apply_login_reply`]
//! is where a login reply is judged, so who is logged in is decided
//! in code a unit test can drive.
//!
//! The shapes are what the example's QML reads: a page reply is an
//! object whose `data` is the row array (the QML binds
//! `resource.data` as a model and reads `data[i].email` etc.), and
//! the row fields are whatever the server sent — the resource type
//! is used for both users and colors, so rows stay JSON values and
//! only the envelope is typed. The things no QML file can show are
//! marked where they stand: how the page number reaches the server
//! (`?page=N` is the convention chosen), the envelope field that
//! carries the page count (`total_pages`, with a fallback when a
//! server does not send it), and the user a login reply names
//! (see [`Session::apply_login_reply`]).

use ljos::prelude::{
    FetchError, Method as PlatformMethod, Request as PlatformRequest, Response as PlatformResponse,
};
use serde::Deserialize;
use serde_json::Value as Json;

/// What the core can say went wrong, in terms a caller can show.
#[derive(Debug, thiserror::Error)]
pub enum RestError {
    /// No server was chosen yet, so there is nowhere to ask.
    #[error("no server URL set")]
    NoBaseUrl,
    /// The reply was not the JSON shape the QML needs.
    #[error("reply is not a page of data: {0}")]
    BadReply(#[from] serde_json::Error),
    /// The platform said the request failed, in its own words.
    #[error("{0}")]
    Network(String),
    /// The request travelled and the server refused it: any answer
    /// outside 2xx is a failure the caller can say in one line.
    #[error("the server answered HTTP {0}")]
    HttpStatus(u16),
}

/// An HTTP method, as far as this client needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        })
    }
}

/// The server's address every request of one service starts from,
/// e.g. `https://reqres.in/api` (the address ServerSelection.qml
/// writes to `RestService.url`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(String);

impl BaseUrl {
    pub fn new(url: impl Into<String>) -> Result<Self, RestError> {
        let url = url.into();
        if url.trim().is_empty() {
            return Err(RestError::NoBaseUrl);
        }
        Ok(BaseUrl(url))
    }

    /// `base` and `path` joined with exactly one slash, however the
    /// two were written.
    pub fn join(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.0.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// The address of one page of `path`. How a page number is
    /// asked for is not written in any QML file; `?page=N` is the
    /// convention this core commits to.
    pub fn page(&self, path: &str, page: i32) -> String {
        format!("{}?page={page}", self.join(path))
    }
}

/// One request, as a value: built by the core, carried by whoever
/// owns a transport.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub body: Option<Json>,
}

impl Request {
    /// This request as the platform's request value, ready for
    /// `Platform::request`: the method and the URL as they stand,
    /// and the JSON body, when there is one, through the seam's
    /// JSON builder so it travels with its content type. The core
    /// keeps its own [`Method`] and [`Request`] because a request
    /// as a core value is this crate's shape; this is the one
    /// place the two vocabularies meet.
    pub fn to_platform(&self) -> PlatformRequest {
        let method = match self.method {
            Method::Get => PlatformMethod::Get,
            Method::Post => PlatformMethod::Post,
            Method::Put => PlatformMethod::Put,
            Method::Delete => PlatformMethod::Delete,
        };
        let request = PlatformRequest::new(method, self.url.as_str());
        match &self.body {
            Some(body) => request.json(&body.to_string()),
            None => request,
        }
    }

    fn new(method: Method, url: String, body: Option<Json>) -> Self {
        Request { method, url, body }
    }
}

/// One page of a resource, decoded: the rows the QML shows, and
/// which page of how many they are.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub rows: Vec<Json>,
    pub page: i32,
    pub pages: i32,
}

/// The reply envelope as the QML's use of it implies: `data` is the
/// rows. `total_pages` names the page count; no QML file can show
/// that field name, so it is read when present and the page the
/// reply claims (or the one asked for) stands alone when absent.
#[derive(Deserialize)]
struct Envelope {
    data: Vec<Json>,
    #[serde(default)]
    page: Option<i32>,
    #[serde(default)]
    total_pages: Option<i32>,
}

/// Where a [`Paginated`] resource stands: the page it shows, how
/// many there are, and the rows of the page it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Paginated {
    path: String,
    page: i32,
    pages: i32,
    rows: Vec<Json>,
}

impl Paginated {
    pub fn new(path: impl Into<String>) -> Self {
        Paginated {
            path: path.into(),
            // Pages are numbered from 1 in the QML (the page
            // buttons are `index + 1`), and nothing is known to
            // exist until a reply says so.
            page: 1,
            pages: 0,
            rows: Vec::new(),
        }
    }

    pub fn set_path(&mut self, path: impl Into<String>) {
        self.path = path.into();
    }

    pub fn page(&self) -> i32 {
        self.page
    }

    pub fn pages(&self) -> i32 {
        self.pages
    }

    pub fn rows(&self) -> &[Json] {
        &self.rows
    }

    /// Asking for the page that is already shown asks again
    /// (`refreshCurrentPage`), so the request is built either way.
    pub fn request_page(&self, base: &BaseUrl, page: i32) -> Request {
        Request::new(Method::Get, base.page(&self.path, page.max(1)), None)
    }

    pub fn request_current(&self, base: &BaseUrl) -> Request {
        self.request_page(base, self.page)
    }

    /// A row to add, as the QML hands it over (`add({name, color,
    /// pantone_value})`): a POST of the row to the resource path.
    pub fn request_add(&self, base: &BaseUrl, row: Json) -> Request {
        Request::new(Method::Post, base.join(&self.path), Some(row))
    }

    /// A row to replace (`update({…}, cid)`): a PUT to the row's
    /// own address under the resource path.
    pub fn request_update(&self, base: &BaseUrl, row: Json, id: i32) -> Request {
        Request::new(
            Method::Put,
            base.join(&format!("{}/{id}", self.path)),
            Some(row),
        )
    }

    /// A row to remove (`remove(cid)`).
    pub fn request_remove(&self, base: &BaseUrl, id: i32) -> Request {
        Request::new(
            Method::Delete,
            base.join(&format!("{}/{id}", self.path)),
            None,
        )
    }

    /// Moves to the page `bytes` answers for. A reply that does
    /// not decode changes nothing and says why.
    pub fn apply_reply(&mut self, asked: i32, bytes: &[u8]) -> Result<Page, RestError> {
        let envelope: Envelope = serde_json::from_slice(bytes)?;
        self.page = envelope.page.unwrap_or(asked).max(1);
        self.pages = envelope.total_pages.unwrap_or(self.page).max(self.page);
        self.rows = envelope.data;
        Ok(Page {
            rows: self.rows.clone(),
            page: self.page,
            pages: self.pages,
        })
    }
}

/// Who is logged in, if anyone: the whole session state the QML
/// reads (`loggedIn`, and `user` compared against a row's `email`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    user: Option<String>,
}

impl Session {
    pub fn logged_in(&self) -> bool {
        self.user.is_some()
    }

    pub fn user(&self) -> &str {
        self.user.as_deref().unwrap_or("")
    }

    /// The login the QML asks for (`login({email, password, id})`):
    /// the credentials are posted whole, as given, and the `email`
    /// in them is who the session will be for if the server agrees.
    /// The second return is that pending user, kept until the
    /// reply decides.
    pub fn request_login(
        &self,
        base: &BaseUrl,
        login_path: &str,
        credentials: Json,
    ) -> (Request, Option<String>) {
        let pending = credentials
            .get("email")
            .and_then(Json::as_str)
            .map(str::to_owned);
        (
            Request::new(Method::Post, base.join(login_path), Some(credentials)),
            pending,
        )
    }

    /// Judges the reply to the login [`Request`] — the whole rule
    /// behind what the QML reads as `loggedIn` and `user`:
    ///
    /// - A 2xx answer logs in. Who as follows the core's fallback
    ///   style (the page envelope's `total_pages` is read the same
    ///   way): the reply's own user fields win when it sends them —
    ///   an `email`, at the top of the reply object or under its
    ///   `user` — and the `email` the credentials carried
    ///   (`pending`) stands when the reply names nobody, as a
    ///   reply carrying only a token does. A reply body that is
    ///   not a JSON object names nobody either; the 2xx still
    ///   counts. With neither, there is nobody to be and the
    ///   session stays logged out.
    /// - Any other answer, and a request that never travelled,
    ///   leaves the session exactly as it was and says why, so the
    ///   caller can log the failure in one line. The QML only
    ///   offers login while logged out, so in that flow a failed
    ///   login is simply still logged out, and a non-2xx reply can
    ///   never set `loggedIn`.
    pub fn apply_login_reply(
        &mut self,
        pending: Option<String>,
        reply: Result<&PlatformResponse, &FetchError>,
    ) -> Result<(), RestError> {
        let response = match reply {
            Ok(response) => response,
            Err(err) => return Err(RestError::Network(err.to_string())),
        };
        if !response.ok() {
            return Err(RestError::HttpStatus(response.status));
        }
        self.user = reply_user(&response.body).or(pending);
        Ok(())
    }

    pub fn request_logout(&self, base: &BaseUrl, logout_path: &str) -> Request {
        Request::new(Method::Post, base.join(logout_path), None)
    }

    /// Logging out is the client's own act: whoever was logged in
    /// no longer is, whatever the server makes of the request.
    pub fn apply_logout(&mut self) {
        self.user = None;
    }
}

/// The user a login reply's body names, in the reply's own fields:
/// an `email` at the top, an `email` under `user`, or `user` itself
/// as a string. Nothing readable there — missing fields, an empty
/// body, a body that is not a JSON object — names nobody, and the
/// caller falls back to the credentials' `email`.
fn reply_user(body: &[u8]) -> Option<String> {
    let json: Json = serde_json::from_slice(body).ok()?;
    json.get("email")
        .and_then(Json::as_str)
        .or_else(|| {
            json.get("user")
                .and_then(|user| user.get("email"))
                .and_then(Json::as_str)
        })
        .or_else(|| json.get("user").and_then(Json::as_str))
        .filter(|user| !user.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn base() -> BaseUrl {
        // The public test server ServerSelection.qml offers.
        BaseUrl::new("https://reqres.in/api").unwrap()
    }

    /// The JSON content type is part of the conversion's contract,
    /// however the seam capitalises the header's name.
    fn assert_json(request: &PlatformRequest, body: &Json) {
        assert_eq!(request.body, body.to_string().into_bytes(), "{request:?}");
        let content_type = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.as_str());
        assert!(
            matches!(content_type, Some(value) if value.contains("application/json")),
            "{request:?}"
        );
    }

    fn reply(status: u16, body: &str) -> PlatformResponse {
        PlatformResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn joining_puts_exactly_one_slash_between_base_and_path() {
        assert_eq!(base().join("users"), "https://reqres.in/api/users");
        assert_eq!(base().join("/users"), "https://reqres.in/api/users");
        let trailed = BaseUrl::new("http://127.0.0.1:49425/api/").unwrap();
        assert_eq!(
            trailed.join("unknown"),
            "http://127.0.0.1:49425/api/unknown"
        );
        assert_eq!(
            trailed.join("/unknown"),
            "http://127.0.0.1:49425/api/unknown"
        );
    }

    #[test]
    fn an_empty_base_is_the_no_server_chosen_error() {
        assert!(matches!(BaseUrl::new(""), Err(RestError::NoBaseUrl)));
        assert!(matches!(BaseUrl::new("  "), Err(RestError::NoBaseUrl)));
    }

    #[test]
    fn page_requests_are_gets_the_seam_can_carry() {
        let colors = Paginated::new("unknown");
        let request = colors.request_page(&base(), 2);
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.body, None);
        assert_eq!(request.url, "https://reqres.in/api/unknown?page=2");

        let platform = request.to_platform();
        assert_eq!(platform.method, PlatformMethod::Get);
        assert_eq!(platform.url, "https://reqres.in/api/unknown?page=2");
        assert!(platform.body.is_empty(), "{platform:?}");
        // Page numbers under 1 mean the first page.
        assert_eq!(
            colors.request_page(&base(), 0).url,
            "https://reqres.in/api/unknown?page=1"
        );
    }

    /// Rows carrying exactly the fields ColorView.qml's delegate
    /// and ColorDialogEditor read: id, name, color, pantone_value.
    const COLOR_PAGE_1: &[u8] = br##"{
        "page": 1, "total_pages": 2,
        "data": [
            {"id": 1, "name": "cerulean", "color": "#98B2D1", "pantone_value": "15-4020"},
            {"id": 2, "name": "fuchsia rose", "color": "#C74375", "pantone_value": "17-2031"}
        ]
    }"##;

    const COLOR_PAGE_2: &[u8] = br##"{
        "page": 2, "total_pages": 2,
        "data": [
            {"id": 3, "name": "true red", "color": "#BF1932", "pantone_value": "19-1664"}
        ]
    }"##;

    #[test]
    fn walking_pages_over_canned_replies() {
        let mut colors = Paginated::new("unknown");
        assert_eq!(
            (colors.page(), colors.pages(), colors.rows().len()),
            (1, 0, 0)
        );

        let first = colors.apply_reply(1, COLOR_PAGE_1).unwrap();
        assert_eq!((first.page, first.pages), (1, 2));
        assert_eq!(colors.rows()[0]["name"], json!("cerulean"));
        assert_eq!(colors.rows()[1]["pantone_value"], json!("17-2031"));

        let second = colors.apply_reply(2, COLOR_PAGE_2).unwrap();
        assert_eq!((second.page, second.pages, second.rows.len()), (2, 2, 1));
        assert_eq!(colors.rows()[0]["color"], json!("#BF1932"));

        // Refreshing asks for the page the last reply settled on.
        assert_eq!(
            colors.request_current(&base()).url,
            "https://reqres.in/api/unknown?page=2"
        );
    }

    #[test]
    fn user_rows_keep_the_fields_the_qml_reads() {
        // UserMenu.qml and ColorView.qml read id, email, avatar.
        const USERS: &[u8] = br#"{
            "page": 1, "total_pages": 1,
            "data": [
                {"id": 1, "email": "ada@example.org", "avatar": "https://x/ada.png"},
                {"id": 2, "email": "bo@example.org", "avatar": "https://x/bo.png"}
            ]
        }"#;
        let mut users = Paginated::new("users");
        users.apply_reply(1, USERS).unwrap();
        let row = users
            .rows()
            .iter()
            .find(|r| r["email"] == json!("bo@example.org"))
            .unwrap();
        assert_eq!(row["avatar"], json!("https://x/bo.png"));
        assert_eq!(row["id"], json!(2));
    }

    #[test]
    fn a_reply_without_a_page_count_stands_on_its_own_page() {
        const BARE: &[u8] = br#"{"data": []}"#;
        let mut users = Paginated::new("users");
        let page = users.apply_reply(3, BARE).unwrap();
        assert_eq!((page.page, page.pages), (3, 3));
    }

    #[test]
    fn a_reply_that_is_not_a_page_changes_nothing() {
        let mut colors = Paginated::new("unknown");
        colors.apply_reply(1, COLOR_PAGE_1).unwrap();
        let before = colors.clone();
        assert!(matches!(
            colors.apply_reply(2, b"<html>503</html>"),
            Err(RestError::BadReply(_))
        ));
        assert!(matches!(
            colors.apply_reply(2, br#"{"nope": true}"#),
            Err(RestError::BadReply(_))
        ));
        assert_eq!(colors, before);
    }

    #[test]
    fn write_requests_carry_method_address_and_body() {
        let colors = Paginated::new("unknown");
        let row = json!({"name": "new", "color": "#41cd52", "pantone_value": "PMS 802C"});

        let add = colors.request_add(&base(), row.clone());
        assert_eq!(
            (add.method, add.url.as_str()),
            (Method::Post, "https://reqres.in/api/unknown")
        );
        assert_eq!(add.body, Some(row.clone()));

        let update = colors.request_update(&base(), row, 7);
        assert_eq!(
            (update.method, update.url.as_str()),
            (Method::Put, "https://reqres.in/api/unknown/7")
        );

        let remove = colors.request_remove(&base(), 7);
        assert_eq!(
            (remove.method, remove.url.as_str(), remove.body),
            (Method::Delete, "https://reqres.in/api/unknown/7", None)
        );
    }

    #[test]
    fn write_requests_convert_to_the_platform_seam() {
        let colors = Paginated::new("unknown");
        let row = json!({"name": "new", "color": "#41cd52", "pantone_value": "PMS 802C"});

        let add = colors.request_add(&base(), row.clone()).to_platform();
        assert_eq!(
            (add.method, add.url.as_str()),
            (PlatformMethod::Post, "https://reqres.in/api/unknown")
        );
        assert_json(&add, &row);

        let update = colors.request_update(&base(), row.clone(), 7).to_platform();
        assert_eq!(
            (update.method, update.url.as_str()),
            (PlatformMethod::Put, "https://reqres.in/api/unknown/7")
        );
        assert_json(&update, &row);

        // A removal has no row to send: no body, whatever the
        // platform request otherwise carries.
        let remove = colors.request_remove(&base(), 7).to_platform();
        assert_eq!(
            (remove.method, remove.url.as_str()),
            (PlatformMethod::Delete, "https://reqres.in/api/unknown/7")
        );
        assert!(remove.body.is_empty(), "{remove:?}");
    }

    #[test]
    fn login_state_moves_on_injected_replies() {
        let mut session = Session::default();
        assert!(!session.logged_in() && session.user().is_empty());

        // The credentials object exactly as UserMenu.qml builds it.
        let credentials = json!({"email": "ada@example.org", "password": "apassword", "id": 1});
        let (request, pending) = session.request_login(&base(), "login", credentials.clone());
        assert_eq!(
            (request.method, request.url.as_str()),
            (Method::Post, "https://reqres.in/api/login")
        );
        assert_eq!(request.body, Some(credentials.clone()));

        let platform = request.to_platform();
        assert_eq!(
            (platform.method, platform.url.as_str()),
            (PlatformMethod::Post, "https://reqres.in/api/login")
        );
        assert_json(&platform, &credentials);

        // A 2xx reply that names nobody (a token only) logs the
        // credentials' `email` in: the pending user is the user.
        session
            .apply_login_reply(pending, Ok(&reply(200, r#"{"token": "abc"}"#)))
            .unwrap();
        assert!(session.logged_in());
        assert_eq!(session.user(), "ada@example.org");

        let logout = session.request_logout(&base(), "logout");
        assert_eq!(logout.url, "https://reqres.in/api/logout");
        session.apply_logout();
        assert!(!session.logged_in() && session.user().is_empty());
    }

    #[test]
    fn credentials_without_an_email_log_nobody_in() {
        let mut session = Session::default();
        let (_request, pending) =
            session.request_login(&base(), "login", json!({"password": "apassword"}));
        assert_eq!(pending, None);
        // Even agreed to, with a reply naming nobody either: the
        // 2xx is not an error, and nobody is logged in.
        session
            .apply_login_reply(pending, Ok(&reply(200, r#"{"token": "abc"}"#)))
            .unwrap();
        assert!(!session.logged_in());
    }

    #[test]
    fn a_login_replys_own_user_fields_win() {
        let mut session = Session::default();
        let pending = Some("ada@example.org".to_owned());
        session
            .apply_login_reply(
                pending.clone(),
                Ok(&reply(200, r#"{"email": "reply@example.org"}"#)),
            )
            .unwrap();
        assert!(session.logged_in());
        assert_eq!(session.user(), "reply@example.org");

        let mut nested = Session::default();
        nested
            .apply_login_reply(
                pending,
                Ok(&reply(201, r#"{"user": {"email": "nested@example.org"}}"#)),
            )
            .unwrap();
        assert_eq!(nested.user(), "nested@example.org");

        // And a reply can name the user the credentials could not.
        let mut unnamed = Session::default();
        unnamed
            .apply_login_reply(None, Ok(&reply(200, r#"{"user": "named@example.org"}"#)))
            .unwrap();
        assert_eq!(unnamed.user(), "named@example.org");
    }

    #[test]
    fn a_refused_login_stays_logged_out_and_says_why() {
        let mut session = Session::default();
        let refused = session.apply_login_reply(
            Some("ada@example.org".to_owned()),
            Ok(&reply(401, r#"{"error": "nope"}"#)),
        );
        assert!(
            matches!(&refused, Err(RestError::HttpStatus(401))),
            "{refused:?}"
        );
        // The refusal is sayable in the one line a log gets, and a
        // non-2xx reply never sets the user, reply fields or not.
        assert_eq!(
            refused.unwrap_err().to_string(),
            "the server answered HTTP 401"
        );
        assert!(!session.logged_in() && session.user().is_empty());
    }

    #[test]
    fn a_login_that_never_travelled_stays_logged_out() {
        let mut session = Session::default();
        let failed = FetchError::Failed("could not connect".to_owned());
        let outcome = session.apply_login_reply(Some("ada@example.org".to_owned()), Err(&failed));
        assert!(
            matches!(&outcome, Err(RestError::Network(why)) if why.contains("could not connect")),
            "{outcome:?}"
        );
        assert!(!session.logged_in() && session.user().is_empty());
    }
}
