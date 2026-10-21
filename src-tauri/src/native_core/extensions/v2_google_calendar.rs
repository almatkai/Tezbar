//! Native Google Calendar / People adapter. Behavior attributed to the installed
//! google-calendar 1.4.4 package's .sc-build/{create-event,create-quick-event,
//! search-contacts,list-events,list-calendars}.js (read-only source).
//! OAuth uses a user-owned Google Desktop OAuth client, NOT Raycast's client ID.
use super::v2_sdk;
use crate::native_core::common::{CoreContext, CoreResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[cfg(test)]
const SLUG: &str = "google-calendar";
const API: &str = "https://www.googleapis.com";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPES: &str = "https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/contacts.readonly https://www.googleapis.com/auth/contacts.other.readonly https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile openid";
const PAGE_LIMIT: usize = 100;

pub fn commands() -> &'static [&'static str] {
    &[
        "create-event",
        "create-quick-event",
        "search-contacts",
        "list-events",
        "list-calendars",
    ]
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    s(&v[key])
}
fn node(kind: &str, props: Value, children: Vec<Value>) -> Value {
    json!({"type":kind,"props":props,"children":children})
}
fn action(id: &str, title: &str) -> Value {
    json!({"id":id,"title":title,"kind":"action"})
}
fn form_field(kind: &str, id: &str, title: &str, value: Value) -> Value {
    node(kind, json!({"id":id,"title":title,"value":value}), vec![])
}
fn list(title: &str, rows: Vec<Value>, actions: Vec<Value>, more: bool, search: bool) -> Value {
    let mut result = v2_sdk::view(
        title,
        node(
            "List",
            json!({"__hasMore":more,"__hasServerSearch":search,"searchBarPlaceholder":if search {"Search contacts..."} else {"Search events..."}}),
            rows,
        ),
        actions,
    );
    result["hasMore"] = json!(more);
    result
}
fn auth_view() -> Value {
    v2_sdk::view(
        "Connect Google Calendar",
        node(
            "Detail",
            json!({"markdown":"Connect your own Google Desktop OAuth client to access Calendar and Contacts. Set TEZBAR_GOOGLE_OAUTH_CLIENT_ID to your Desktop application's OAuth client ID, then select Connect. The installed extension's Raycast client ID is registered for Raycast redirects and cannot be assumed to accept Tezbar's loopback callback. Existing Raycast tokens are not imported."}),
            vec![],
        ),
        vec![action("google:connect", "Connect Google")],
    )
}
fn auth_file(ctx: &CoreContext) -> PathBuf {
    // Separate from the extension SDK namespace, which can expose JSON to adapters.
    ctx.paths
        .data_dir
        .join("native-google-oauth")
        .join("tokens.json")
}
fn read_tokens(ctx: &CoreContext) -> Result<Value, String> {
    let path = auth_file(ctx);
    match std::fs::read(&path) {
        Ok(data) => {
            let value: Value = serde_json::from_slice(&data)
                .map_err(|_| "Google OAuth credential file is malformed".to_string())?;
            if !value.is_object() {
                return Err("Google OAuth credential file is not an object".into());
            }
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("Cannot read Google credentials: {e}")),
    }
}
fn save_tokens(ctx: &CoreContext, value: &Value) -> Result<(), String> {
    let path = auth_file(ctx);
    let dir = path.parent().ok_or("Invalid Google credential path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn client_id() -> Result<String, String> {
    let id = std::env::var("TEZBAR_GOOGLE_OAUTH_CLIENT_ID").unwrap_or_default();
    if !id.ends_with(".apps.googleusercontent.com") || id.contains(char::is_whitespace) {
        return Err("Set TEZBAR_GOOGLE_OAUTH_CLIENT_ID to a Google Desktop OAuth client ID before connecting".into());
    }
    Ok(id)
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(StdDuration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())
}
async fn response_json(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Google response was not JSON: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "Google API {}: {}",
            status.as_u16(),
            field(&value["error"], "message")
                .to_string()
                .is_empty()
                .then(|| s(&value["error"]).to_string())
                .unwrap_or_else(|| field(&value["error"], "message").to_string())
        ));
    }
    Ok(value)
}
async fn token_request(
    http: &reqwest::Client,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<Value, String> {
    response_json(
        http.post(endpoint)
            .form(params)
            .send()
            .await
            .map_err(|e| format!("Google OAuth request failed: {e}"))?,
    )
    .await
}
async fn access_token(
    ctx: &CoreContext,
    http: &reqwest::Client,
    token_url: &str,
) -> Result<Option<String>, String> {
    let stored = read_tokens(ctx)?;
    let current = field(&stored, "access_token");
    if !current.is_empty()
        && stored["expires_at"].as_i64().unwrap_or(0) > Utc::now().timestamp() + 90
    {
        return Ok(Some(current.to_owned()));
    }
    let refresh = field(&stored, "refresh_token").to_owned();
    if refresh.is_empty() {
        return Ok(None);
    }
    let id = client_id()?;
    refresh_access_token(ctx, http, token_url, &id, stored).await
}

async fn refresh_access_token(
    ctx: &CoreContext,
    http: &reqwest::Client,
    token_url: &str,
    id: &str,
    mut stored: Value,
) -> Result<Option<String>, String> {
    let refresh = field(&stored, "refresh_token").to_owned();
    if refresh.is_empty() {
        return Ok(None);
    }
    let response = http
        .post(token_url)
        .form(&[
            ("client_id", id),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("Google OAuth refresh request failed: {e}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Google OAuth refresh response was not JSON: {e}"))?;
    if status == reqwest::StatusCode::BAD_REQUEST && field(&value, "error") == "invalid_grant" {
        // A revoked/expired grant needs fresh consent, not an endless error loop.
        // Never discard newer credentials written by another session meanwhile.
        let current = read_tokens(ctx)?;
        if field(&current, "refresh_token").is_empty() {
            return Ok(None);
        }
        if current["refresh_token"] != stored["refresh_token"]
            || current["access_token"] != stored["access_token"]
        {
            return Err("Google credentials changed during refresh; retry the command".into());
        }
        save_tokens(ctx, &json!({}))?;
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!(
            "Google OAuth refresh failed ({}): {}",
            status.as_u16(),
            value["error_description"]
                .as_str()
                .or(value["error"]["message"].as_str())
                .or(value["error"].as_str())
                .unwrap_or("Unknown OAuth error")
        ));
    }
    let response = value;
    let access = field(&response, "access_token");
    if access.is_empty() {
        return Err("Google OAuth refresh returned no access token".into());
    }
    stored["access_token"] = json!(access);
    stored["expires_at"] =
        json!(Utc::now().timestamp() + response["expires_in"].as_i64().unwrap_or(3600));
    if let Some(new_refresh) = response["refresh_token"].as_str() {
        stored["refresh_token"] = json!(new_refresh);
    }
    save_tokens(ctx, &stored)?;
    Ok(Some(access.to_owned()))
}

// OAuth 2.0 Authorization Code + PKCE loopback. The listener is bound BEFORE the
// browser opens; state and callback path are checked before the code is exchanged.
async fn connect(ctx: &CoreContext, http: &reqwest::Client, token_url: &str) -> Result<(), String> {
    let id = client_id()?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Cannot open OAuth loopback listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/oauth/google");
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = uuid::Uuid::new_v4().to_string();
    let auth = format!("https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
        v2_sdk::encode_component(&id), v2_sdk::encode_component(&redirect), v2_sdk::encode_component(SCOPES), challenge, state);
    v2_sdk::open(ctx, &auth).await?;
    let code = tokio::time::timeout(StdDuration::from_secs(180), async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut data = vec![0; 8192];
            let n = tokio::time::timeout(StdDuration::from_secs(5), stream.read(&mut data)).await.map_err(|_| "OAuth callback timed out".to_string())?.map_err(|e| e.to_string())?;
            let first = String::from_utf8_lossy(&data[..n]);
            let Some(path) = first.lines().next().and_then(|l| l.strip_prefix("GET ")).and_then(|l| l.split(' ').next()) else { continue; };
            let url = match reqwest::Url::parse(&format!("http://127.0.0.1:{port}{path}")) { Ok(u) => u, Err(_) => continue };
            if url.path() != "/oauth/google" { continue; }
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().map(|(k,v)| (k.into_owned(),v.into_owned())).collect();
            let valid = pairs.get("state") == Some(&state);
            let reply = if valid { "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nAuthorization received. Return to Tezbar." } else { "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nInvalid OAuth state." };
            let _ = stream.write_all(reply.as_bytes()).await;
            if !valid { continue; }
            if let Some(error) = pairs.get("error") { return Err(format!("Google authorization denied: {error}")); }
            return pairs.get("code").cloned().ok_or_else(|| "Google callback omitted authorization code".into());
        }
    }).await.map_err(|_| "Google authorization timed out".to_string())??;
    let token = token_request(
        http,
        token_url,
        &[
            ("code", &code),
            ("client_id", &id),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("grant_type", "authorization_code"),
        ],
    )
    .await?;
    if field(&token, "access_token").is_empty() || field(&token, "refresh_token").is_empty() {
        return Err(
            "Google OAuth did not grant offline access; reconnect and allow consent".into(),
        );
    }
    save_tokens(
        ctx,
        &json!({"access_token":token["access_token"],"refresh_token":token["refresh_token"],"expires_at":Utc::now().timestamp()+token["expires_in"].as_i64().unwrap_or(3600)}),
    )
}

struct Google<'a> {
    http: &'a reqwest::Client,
    base: &'a str,
    token: &'a str,
}
impl Google<'_> {
    async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value, String> {
        response_json(
            self.http
                .get(format!("{}{path}", self.base))
                .bearer_auth(self.token)
                .query(params)
                .send()
                .await
                .map_err(|e| format!("Google API request failed: {e}"))?,
        )
        .await
    }
    async fn post(
        &self,
        path: &str,
        params: &[(&str, String)],
        body: &Value,
    ) -> Result<Value, String> {
        response_json(
            self.http
                .post(format!("{}{path}", self.base))
                .bearer_auth(self.token)
                .query(params)
                .json(body)
                .send()
                .await
                .map_err(|e| format!("Google API request failed: {e}"))?,
        )
        .await
    }
    async fn patch(&self, path: &str, body: &Value) -> Result<Value, String> {
        response_json(
            self.http
                .patch(format!("{}{path}", self.base))
                .bearer_auth(self.token)
                .json(body)
                .send()
                .await
                .map_err(|e| format!("Google API request failed: {e}"))?,
        )
        .await
    }
    async fn delete(&self, path: &str) -> Result<(), String> {
        let response = self
            .http
            .delete(format!("{}{path}", self.base))
            .bearer_auth(self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Google API delete failed: {}", response.status()));
        }
        Ok(())
    }
    async fn pages(&self, path: &str, params: &[(&str, String)]) -> Result<Vec<Value>, String> {
        let mut all = Vec::new();
        let mut cursor = String::new();
        for _ in 0..PAGE_LIMIT {
            let mut query = params.to_vec();
            if !cursor.is_empty() {
                query.push(("pageToken", cursor.clone()));
            }
            let response = self.get(path, &query).await?;
            all.extend(response["items"].as_array().cloned().unwrap_or_default());
            cursor = field(&response, "nextPageToken").to_string();
            if cursor.is_empty() {
                return Ok(all);
            }
        }
        Err("Google pagination exceeded 100 pages; results are not complete".into())
    }
}
fn cal_path(id: &str) -> String {
    format!(
        "/calendar/v3/calendars/{}/events",
        v2_sdk::encode_component(id)
    )
}
fn item(
    id: String,
    title: &str,
    subtitle: &str,
    action_ids: Vec<String>,
    accessories: Vec<Value>,
) -> Value {
    node(
        "List.Item",
        json!({"id":id,"title":title,"subtitle":subtitle,"actionIds":action_ids,"accessories":accessories}),
        vec![],
    )
}
fn set_items(state: &mut Value, items: Vec<Value>) {
    state["items"] = json!(items);
}
fn add_items(state: &mut Value, items: Vec<Value>) {
    if !state["items"].is_array() {
        state["items"] = json!([]);
    }
    state["items"].as_array_mut().unwrap().extend(items);
}
fn render(command: &str, state: &Value) -> Value {
    let command = if command == "list-calendars" && state["calendarView"] == true {
        "list-events"
    } else {
        command
    };
    let items = state["items"].as_array().cloned().unwrap_or_default();
    match command {
        "list-calendars" => {
            let mut rows = Vec::new();
            let mut actions = Vec::new();
            for (i, c) in items.iter().enumerate() {
                let id = field(c, "id");
                if id.is_empty() {
                    continue;
                }
                let name = c["summaryOverride"]
                    .as_str()
                    .or(c["summary"].as_str())
                    .unwrap_or("Untitled Calendar");
                let a = format!("google:calendar:{i}");
                rows.push(item(id.into(),name,field(c,"description"),vec![a.clone()],vec![json!({"tag":if c["primary"] == true {"Primary"} else if c["selected"] == true {"Visible"} else {"Hidden"}}),json!({"tag":field(c,"accessRole")})]));
                actions.push(action(&a, "Show Events"));
            }
            list("Google Calendars", rows, actions, false, false)
        }
        "list-events" => {
            let mut rows = Vec::new();
            let mut actions = Vec::new();
            for (i, e) in items.iter().enumerate() {
                let open = format!("google:event:open:{i}");
                let copy = format!("google:event:copy:{i}");
                let copy_title = format!("google:event:title:{i}");
                let copy_meeting = format!("google:event:copy-meeting:{i}");
                let del = format!("google:event:delete:{i}");
                let meet = format!("google:event:meet:{i}");
                let respond = format!("google:event:respond:{i}");
                let meeting = field(&e["conferenceData"]["entryPoints"][0], "uri");
                let primary = if state["openCommandJoinsMeeting"] == true && !meeting.is_empty() {
                    meet.clone()
                } else if !field(e, "htmlLink").is_empty() {
                    open.clone()
                } else if !meeting.is_empty() {
                    meet.clone()
                } else {
                    copy_title.clone()
                };
                let mut ids = vec![primary];
                let mut accessories = Vec::new();
                if state["showLocation"] == true && !field(e, "location").is_empty() {
                    accessories.push(json!({"text":e["location"],"icon":"Pin"}));
                }
                if e["recurringEventId"].is_string() || e["recurrence"].is_array() {
                    accessories.push(json!({"icon":"Repeat"}));
                }
                if let Some(attendees) = e["attendees"].as_array() {
                    accessories.push(json!({"text":attendees.len().to_string(),"icon":"Person"}));
                }
                if !meeting.is_empty() {
                    ids.push(meet.clone());
                    actions.push(action(&meet, "Join Meeting"));
                    ids.push(copy_meeting.clone());
                    actions.push(action(&copy_meeting, "Copy Meeting Link"));
                }
                if !field(e, "htmlLink").is_empty() {
                    ids.push(open.clone());
                    actions.push(action(&open, "Open in Google Calendar"));
                    ids.push(copy.clone());
                    actions.push(action(&copy, "Copy Event Link"));
                }
                ids.push(copy_title.clone());
                actions.push(action(&copy_title, "Copy Event Title"));
                if !field(e, "id").is_empty() {
                    ids.push(respond.clone());
                    actions.push(action(&respond, "Change Response (Yes / Maybe / No)"));
                    ids.push(del.clone());
                    actions.push(action(&del, "Delete Event"));
                }
                let start = e["start"]["dateTime"]
                    .as_str()
                    .or(e["start"]["date"].as_str())
                    .unwrap_or("");
                let end = e["end"]["dateTime"]
                    .as_str()
                    .or(e["end"]["date"].as_str())
                    .unwrap_or("");
                rows.push(item(
                    format!("{}:{i}", field(e, "id")),
                    e["summary"].as_str().unwrap_or("Untitled Event"),
                    &format!("{start} – {end}"),
                    ids,
                    accessories,
                ));
            }
            let more = !field(state, "nextPageToken").is_empty();
            let mut result = list("Upcoming Events", rows, actions, more, false);
            let mut options = vec![node(
                "List.Dropdown.Item",
                json!({"title":"Primary","value":"primary"}),
                vec![],
            )];
            if let Some(calendars) = state["calendars"].as_array() {
                for c in calendars {
                    let id = field(c, "id");
                    if !id.is_empty() && id != "primary" {
                        options.push(node("List.Dropdown.Item",json!({"title":c["summaryOverride"].as_str().or(c["summary"].as_str()).unwrap_or(id),"value":id}),vec![]));
                    }
                }
            }
            options.push(node(
                "List.Dropdown.Item",
                json!({"title":"Birthdays","value":"birthdays"}),
                vec![],
            ));
            let selected = field(state, "calendar");
            if let Some(position) = options
                .iter()
                .position(|c| field(&c["props"], "value") == selected)
            {
                options.swap(0, position);
            }
            result["root"]["props"]["searchBarAccessory"] = node(
                "List.Dropdown",
                json!({"actionId":"google:calendar-select"}),
                options,
            );
            result
        }
        "search-contacts" => {
            let mut rows = Vec::new();
            let mut actions = Vec::new();
            for (i, c) in items.iter().enumerate() {
                let email = field(&c["emailAddresses"][0], "value");
                if email.is_empty() {
                    continue;
                }
                let a = format!("google:contact:copy:{i}");
                rows.push(item(
                    field(c, "resourceName").into(),
                    email,
                    field(&c["names"][0], "displayName"),
                    vec![a.clone()],
                    vec![],
                ));
                actions.push(action(&a, "Copy Email"));
            }
            list("Search Contacts", rows, actions, false, true)
        }
        _ => v2_sdk::unchanged(),
    }
}
fn create_form(command: &str, calendars: &[Value], prefs: &Value, state: &Value) -> Value {
    let quick = command == "create-quick-event";
    let rounded = (Utc::now().timestamp() + 899).div_euclid(900) * 900;
    let suggested_start = DateTime::<Utc>::from_timestamp(rounded, 0)
        .unwrap_or_else(Utc::now)
        .with_timezone(&chrono::Local)
        .to_rfc3339();
    let fields = if quick {
        vec![form_field(
            "Form.TextField",
            "input",
            "Title and Date/Time",
            json!(field(state, "input")),
        )]
    } else {
        let options:Vec<Value>=calendars.iter().filter(|c| matches!(field(c,"accessRole"),"owner"|"writer") && !field(c,"id").is_empty()).map(|c|json!({"title":c["summaryOverride"].as_str().or(c["summary"].as_str()).unwrap_or("Calendar"),"value":c["id"]})).collect();
        vec![
            node(
                "Form.Dropdown",
                json!({"id":"calendar","title":"Calendar","value":state["calendar"].as_str().unwrap_or("primary"),"data":options}),
                vec![],
            ),
            form_field("Form.TextField", "title", "Title", json!("")),
            form_field("Form.Checkbox", "allDay", "All-day", json!(false)),
            node(
                "Form.TextField",
                json!({"id":"startDate","title":"Start Date and Time","value":suggested_start,"placeholder":"2027-06-15T15:00:00+02:00 (or YYYY-MM-DD for all-day)","info":"Timed events require an ISO 8601 timezone offset (or Z)."}),
                vec![],
            ),
            form_field(
                "Form.DatePicker",
                "endDate",
                "End Date (all-day, inclusive)",
                Value::Null,
            ),
            form_field(
                "Form.TextField",
                "duration",
                "Duration (minutes or 1h30m)",
                json!(prefs["defaultEventDuration"].as_str().unwrap_or("30")),
            ),
            form_field(
                "Form.TextField",
                "attendees",
                "Guests (emails separated by commas)",
                json!(""),
            ),
            form_field(
                "Form.Dropdown",
                "conferencingProvider",
                "Conferencing",
                json!("none"),
            ),
            form_field(
                "Form.TextField",
                "conferenceUrl",
                "Other Meeting URL",
                json!(""),
            ),
            form_field("Form.TextArea", "description", "Description", json!("")),
            form_field(
                "Form.Dropdown",
                "sendInvitations",
                "Send Invitations",
                json!(prefs["sendInvitations"].as_str().unwrap_or("all")),
            ),
        ]
    };
    let fields=fields.into_iter().map(|mut f| { if f["props"]["id"]=="conferencingProvider" {f["props"]["data"]=json!([{"title":"None","value":"none"},{"title":"Google Meet","value":"hangoutsMeet"}]);} if f["props"]["id"]=="sendInvitations" {f["props"]["data"]=json!([{"title":"All guests","value":"all"},{"title":"External guests","value":"externalOnly"},{"title":"None","value":"none"}]);} f }).collect();
    let title = if quick {
        "Create Quick Event"
    } else {
        "Create Event"
    };
    v2_sdk::view(
        title,
        node("Form", json!({}), fields),
        vec![json!({"id":"google:submit","title":title,"kind":"submit-form"})],
    )
}
fn minutes(text: &str) -> Result<i64, String> {
    let input = text.trim().to_lowercase();
    if let Ok(number) = input.parse::<i64>() {
        return (1..=10080)
            .contains(&number)
            .then_some(number)
            .ok_or_else(|| "Duration must be between 1 and 10080 minutes".into());
    }
    let re = regex::Regex::new(r"(\d+)\s*(hours?|hrs?|h|minutes?|mins?|m)").unwrap();
    let mut sum = 0_i64;
    let mut end = 0;
    for cap in re.captures_iter(&input) {
        let matched = cap.get(0).unwrap();
        if !input[end..matched.start()].trim().is_empty() {
            return Err("Invalid duration; use 30, 45m or 1h30m".into());
        }
        let count = cap[1].parse::<i64>().map_err(|e| e.to_string())?;
        sum = sum
            .checked_add(
                count
                    .checked_mul(if cap[2].starts_with('h') { 60 } else { 1 })
                    .ok_or("Duration overflow")?,
            )
            .ok_or("Duration overflow")?;
        end = matched.end();
    }
    if !input[end..].trim().is_empty() || !(1..=10080).contains(&sum) {
        return Err("Invalid duration; use 30, 45m or 1h30m".into());
    }
    Ok(sum)
}
fn day(raw: &str) -> Result<NaiveDate, String> {
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(d);
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.date_naive())
        .map_err(|_| "Date must be YYYY-MM-DD or ISO 8601 with timezone".into())
}
fn date_time(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.to_utc())
        .map_err(|_| "Timed events require a timezone-aware ISO 8601 start date".into())
}
fn event_body(
    values: &Value,
    prefs: &Value,
) -> Result<(String, Value, Vec<(&'static str, String)>), String> {
    let title = field(values, "title").trim();
    if title.is_empty() {
        return Err("Event title is required".into());
    }
    let id = values["calendar"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("primary")
        .to_owned();
    let all_day = values["allDay"].as_bool().unwrap_or(false);
    let (start, end) = if all_day {
        let begin = day(field(values, "startDate"))?;
        let last = if field(values, "endDate").is_empty() {
            begin
        } else {
            day(field(values, "endDate"))?
        };
        if last < begin {
            return Err("All-day end cannot precede start".into());
        }
        (
            json!({"date":begin.to_string()}),
            json!({"date":last.succ_opt().ok_or("Date overflow")?.to_string()}),
        )
    } else {
        let start = date_time(field(values, "startDate"))?;
        let duration = minutes(
            values["duration"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(prefs["defaultEventDuration"].as_str().unwrap_or("30")),
        )?;
        (
            json!({"dateTime":start.to_rfc3339()}),
            json!({"dateTime":(start+Duration::minutes(duration)).to_rfc3339()}),
        )
    };
    let guests = field(values, "attendees")
        .split([',', ';', '\n'])
        .filter(|v| !v.trim().is_empty())
        .map(|v| {
            let address = v
                .trim()
                .rsplit_once('<')
                .and_then(|(_, rest)| rest.strip_suffix('>'))
                .unwrap_or(v.trim())
                .trim();
            let (local, domain) = address
                .split_once('@')
                .ok_or_else(|| format!("Invalid guest email: {address}"))?;
            if local.is_empty() || !domain.contains('.') || address.chars().any(char::is_whitespace)
            {
                return Err(format!("Invalid guest email: {address}"));
            }
            Ok(json!({"email":address}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut description = field(values, "description").to_string();
    if prefs["addSignature"].as_bool().unwrap_or(true) {
        if !description.is_empty() {
            description.push_str("\n\n");
        }
        description.push_str("Created with Tezbar");
    }
    let provider = field(values, "conferencingProvider");
    let mut body = json!({"summary":title,"start":start,"end":end,"description":description});
    if !guests.is_empty() {
        body["attendees"] = json!(guests);
    }
    let mut params = vec![];
    if provider == "hangoutsMeet" {
        body["conferenceData"] = json!({"createRequest":{"conferenceSolutionKey":{"type":"hangoutsMeet"},"requestId":uuid::Uuid::new_v4().to_string()}});
        params.push(("conferenceDataVersion", "1".into()));
    } else if !provider.is_empty() && provider != "none" {
        return Err("Unsupported conferencing provider".into());
    }
    let other_url = field(values, "conferenceUrl").trim();
    if !other_url.is_empty() {
        if provider == "hangoutsMeet" {
            return Err("Choose either Google Meet or another meeting URL".into());
        }
        let parsed =
            reqwest::Url::parse(other_url).map_err(|_| "Invalid meeting URL".to_string())?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err("Meeting URL must be HTTPS".into());
        }
        body["location"] = json!(other_url);
    }
    let send = values["sendInvitations"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or(prefs["sendInvitations"].as_str().unwrap_or("all"));
    if !matches!(send, "all" | "externalOnly" | "none") {
        return Err("Invalid invitation preference".into());
    }
    params.push(("sendUpdates", send.into()));
    Ok((id, body, params))
}
fn https_link(url: &str) -> Result<&str, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Invalid Google link".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("External links must use HTTPS".into());
    }
    Ok(url)
}
fn index(action: &str, prefix: &str) -> Option<usize> {
    action.strip_prefix(prefix)?.parse::<usize>().ok()
}
async fn load_calendars(api: &Google<'_>) -> Result<Vec<Value>, String> {
    api.pages("/calendar/v3/users/me/calendarList", &[]).await
}
fn event_types(cal: &str, birthdays: bool) -> Vec<(&'static str, String)> {
    let types: &[&str] = if cal == "birthdays"
        || cal.contains("addressbook#contacts")
        || cal.contains("#contacts@group")
    {
        &["birthday"]
    } else if birthdays {
        &[
            "default",
            "focusTime",
            "outOfOffice",
            "workingLocation",
            "fromGmail",
            "birthday",
        ]
    } else {
        &[
            "default",
            "focusTime",
            "outOfOffice",
            "workingLocation",
            "fromGmail",
        ]
    };
    types.iter().map(|t| ("eventTypes", (*t).into())).collect()
}
fn events_calendar(cal: &str) -> &str {
    if cal == "birthdays" {
        "primary"
    } else {
        cal
    }
}
async fn refresh(api: &Google<'_>, command: &str, state: &mut Value) -> Result<(), String> {
    match command {
        "list-calendars" => {
            set_items(state, load_calendars(api).await?);
        }
        "list-events" => {
            let cal = state["calendar"].as_str().unwrap_or("primary");
            let mut query = vec![
                ("timeMin", Utc::now().to_rfc3339()),
                ("maxResults", "50".into()),
                ("singleEvents", "true".into()),
                ("orderBy", "startTime".into()),
            ];
            query.extend(event_types(cal, state["showBirthdays"] == true));
            let page = api.get(&cal_path(events_calendar(cal)), &query).await?;
            set_items(state, page["items"].as_array().cloned().unwrap_or_default());
            state["nextPageToken"] = page["nextPageToken"].clone();
            state["timeMin"] = json!(query[0].1);
        }
        "search-contacts" => {
            let query = field(state, "query").trim();
            if query.is_empty() {
                set_items(state, vec![]);
                return Ok(());
            }
            let contact_params = [
                ("query", query.into()),
                ("readMask", "names,emailAddresses,photos".into()),
                ("pageSize", "30".into()),
                ("sources", "READ_SOURCE_TYPE_CONTACT".into()),
                ("sources", "READ_SOURCE_TYPE_PROFILE".into()),
            ];
            let other_params = [
                ("query", query.into()),
                ("readMask", "names,emailAddresses,photos".into()),
                ("pageSize", "30".into()),
            ];
            let (people, other) = tokio::try_join!(
                api.get(
                    "/v1/people:searchContacts",
                    &contact_params
                ),
                api.get(
                    "/v1/otherContacts:search",
                    &other_params
                )
            )?;
            let contacts = people["results"]
                .as_array()
                .into_iter()
                .chain(other["results"].as_array())
                .flatten()
                .filter_map(|r| r.get("person").cloned())
                .collect();
            set_items(state, contacts);
        }
        _ => {}
    }
    Ok(())
}
async fn invoke(
    ctx: &CoreContext,
    api: &Google<'_>,
    command: &str,
    action_id: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    let values = &payload["formValues"];
    let confirming = action_id == "google:confirm";
    let action_id = if confirming {
        let pending = field(state, "pending");
        if pending.is_empty() {
            return Err("No pending Google action".into());
        }
        pending.to_owned()
    } else {
        action_id.to_owned()
    };
    let action_id = action_id.as_str();
    if action_id == "google:submit" {
        let event = if command == "create-quick-event" {
            let text = field(values, "input").trim();
            if text.is_empty() {
                return Err("Enter an event title and date/time".into());
            }
            // Google's quickAdd parses natural-language expressions; no synthetic local result.
            api.post(
                &format!("{}/quickAdd", cal_path("primary")),
                &[("text", text.into())],
                &json!({}),
            )
            .await?
        } else {
            let prefs = &payload["preferences"];
            let (id, body, params) = event_body(values, prefs)?;
            state["calendar"] = json!(id);
            api.post(&cal_path(&id), &params, &body).await?
        };
        if field(&event, "id").is_empty() {
            return Err("Google did not return a created event ID".into());
        }
        state["createdEvent"] = event;
        return Ok(created_view(state));
    }
    if action_id == "google:created:open" {
        let url = field(&state["createdEvent"], "htmlLink");
        if url.is_empty() {
            return Err("Created event has no link".into());
        }
        v2_sdk::open(ctx, https_link(url)?).await?;
        return Ok(created_view(state));
    }
    if action_id == "google:created:delete" {
        if !confirming {
            state["pending"] = json!(action_id);
            return Ok(confirm_view("Delete Created Event", true));
        }
        if values["confirmed"] != true {
            return Err("Confirm deletion to continue".into());
        }
        let id = field(&state["createdEvent"], "id");
        let cal = state["calendar"].as_str().unwrap_or("primary");
        if id.is_empty() {
            return Err("Created event has no ID".into());
        }
        api.delete(&format!(
            "{}/{}",
            cal_path(cal),
            v2_sdk::encode_component(id)
        ))
        .await?;
        state["createdEvent"] = Value::Null;
        state["pending"] = Value::Null;
        return Ok(v2_sdk::no_view("Event deleted"));
    }
    if action_id == "google:calendar-select" {
        if command != "list-events"
            && !(command == "list-calendars" && state["calendarView"] == true)
        {
            return Err("Calendar selection unavailable".into());
        }
        let id = field(values, "value");
        if id != "primary"
            && id != "birthdays"
            && !state["calendars"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| field(c, "id") == id)
        {
            return Err("Unknown calendar".into());
        }
        state["calendar"] = json!(id);
        refresh(api, "list-events", state).await?;
        return Ok(render(command, state));
    }
    if let Some(i) = index(action_id, "google:calendar:") {
        if command != "list-calendars" {
            return Err("Action does not belong to this command".into());
        }
        let selected = state["items"]
            .as_array()
            .and_then(|v| v.get(i))
            .ok_or("Calendar not found")?;
        let id = field(selected, "id").to_owned();
        if id.is_empty() {
            return Err("Calendar has no ID".into());
        }
        state["calendar"] = json!(id);
        state["calendarView"] = json!(true);
        state["calendars"] = json!(load_calendars(api).await?);
        refresh(api, "list-events", state).await?;
        return Ok(render(command, state));
    }
    if let Some(i) = index(action_id, "google:contact:copy:") {
        if command != "search-contacts" {
            return Err("Action does not belong to this command".into());
        }
        let contact = state["items"]
            .as_array()
            .and_then(|v| v.get(i))
            .ok_or("Contact not found")?;
        let email = field(&contact["emailAddresses"][0], "value");
        if email.is_empty() {
            return Err("Contact has no email".into());
        }
        v2_sdk::copy(ctx, email).await?;
        return Ok(render(command, state));
    }
    for (kind, prefix) in [
        ("open", "google:event:open:"),
        ("copy", "google:event:copy:"),
        ("title", "google:event:title:"),
        ("copy-meeting", "google:event:copy-meeting:"),
        ("meet", "google:event:meet:"),
        ("delete", "google:event:delete:"),
        ("respond", "google:event:respond:"),
    ] {
        let Some(i) = index(action_id, prefix) else {
            continue;
        };
        if command != "list-events"
            && !(command == "list-calendars" && state["calendarView"] == true)
        {
            return Err("Action does not belong to this command".into());
        }
        let event = state["items"]
            .as_array()
            .and_then(|v| v.get(i))
            .ok_or("Event not found")?
            .clone();
        if matches!(kind, "delete" | "respond") && !confirming {
            state["pending"] = json!(action_id);
            return Ok(confirm_view(
                if kind == "delete" {
                    "Delete Event"
                } else {
                    "Respond to Event"
                },
                kind == "delete",
            ));
        }
        match kind {
            "open" | "meet" => {
                let target = if kind == "meet" {
                    field(&event["conferenceData"]["entryPoints"][0], "uri")
                } else {
                    field(&event, "htmlLink")
                };
                if target.is_empty() {
                    return Err("Event has no link".into());
                }
                v2_sdk::open(ctx, https_link(target)?).await?;
            }
            "copy" => {
                let url = field(&event, "htmlLink");
                if url.is_empty() {
                    return Err("Event has no link".into());
                }
                v2_sdk::copy(ctx, url).await?;
            }
            "title" => {
                let title = field(&event, "summary");
                if title.is_empty() {
                    return Err("Event has no title".into());
                }
                v2_sdk::copy(ctx, title).await?;
            }
            "copy-meeting" => {
                let uri = field(&event["conferenceData"]["entryPoints"][0], "uri");
                if uri.is_empty() {
                    return Err("Event has no meeting link".into());
                }
                v2_sdk::copy(ctx, uri).await?;
            }
            "delete" => {
                if payload["formValues"]["confirmed"] != true {
                    return Err("Deleting an event requires confirmed=true".into());
                }
                let id = field(&event, "id");
                if id.is_empty() {
                    return Err("Event has no ID".into());
                }
                let cal = state["calendar"].as_str().unwrap_or("primary");
                api.delete(&format!(
                    "{}/{}",
                    cal_path(cal),
                    v2_sdk::encode_component(id)
                ))
                .await?;
                refresh(api, "list-events", state).await?;
            }
            "respond" => {
                let status = field(values, "status");
                if !matches!(status, "accepted" | "tentative" | "declined") {
                    return Err("Specify response status accepted, tentative or declined".into());
                }
                let attendees = event["attendees"]
                    .as_array()
                    .ok_or("Event has no attendees")?;
                let mut attendees = attendees.clone();
                let self_attendee = attendees
                    .iter_mut()
                    .find(|v| v["self"] == true)
                    .ok_or("You are not an attendee")?;
                self_attendee["responseStatus"] = json!(status);
                let id = field(&event, "id");
                let cal = state["calendar"].as_str().unwrap_or("primary");
                api.patch(
                    &format!("{}/{}", cal_path(cal), v2_sdk::encode_component(id)),
                    &json!({"attendees":attendees}),
                )
                .await?;
                refresh(api, "list-events", state).await?;
            }
            _ => unreachable!(),
        }
        state["pending"] = Value::Null;
        return Ok(render(command, state));
    }
    Err(format!("Unknown Google action: {action_id}"))
}
fn confirm_view(title: &str, deleting: bool) -> Value {
    let fields = if deleting {
        vec![form_field(
            "Form.Checkbox",
            "confirmed",
            "Yes, delete this event",
            json!(false),
        )]
    } else {
        vec![node(
            "Form.Dropdown",
            json!({"id":"status","title":"Response","value":"accepted","data":[{"title":"Yes","value":"accepted"},{"title":"Maybe","value":"tentative"},{"title":"No","value":"declined"}]}),
            vec![],
        )]
    };
    v2_sdk::view(
        title,
        node("Form", json!({}), fields),
        vec![json!({"id":"google:confirm","title":title,"kind":"submit-form"})],
    )
}
fn created_view(state: &Value) -> Value {
    let event = &state["createdEvent"];
    let mut actions = vec![];
    if !field(event, "htmlLink").is_empty() {
        actions.push(action("google:created:open", "Open in Google Calendar"));
    }
    if !field(event, "id").is_empty() {
        actions.push(action("google:created:delete", "Delete Created Event"));
    }
    v2_sdk::view(
        "Created Event",
        node(
            "Detail",
            json!({"markdown":format!("# {}\n\n{}",field(event,"summary"),field(event,"htmlLink"))}),
            vec![],
        ),
        actions,
    )
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err(format!("Unknown Google Calendar command: {command}"));
    }
    if operation == "extension:dispose-session" {
        return Ok(v2_sdk::unchanged());
    }
    let http = client()?;
    let action_id = field(payload, "actionId");
    let operation = if operation == "extension:invoke-action" && action_id == "google:connect" {
        connect(ctx, &http, TOKEN_URL).await?;
        "extension:run-command"
    } else {
        operation
    };
    let Some(token) = access_token(ctx, &http, TOKEN_URL).await? else {
        return Ok(auth_view());
    };
    let api = Google {
        http: &http,
        base: API,
        token: &token,
    };
    match operation {
        "extension:run-command" => {
            if command == "create-event" {
                state["calendar"] = json!(payload["launchContext"]["calendar"]
                    .as_str()
                    .unwrap_or("primary"));
            }
            if command == "list-events" {
                state["calendars"] = json!(load_calendars(&api).await?);
                state["calendar"] = json!(payload["launchContext"]["calendarId"]
                    .as_str()
                    .unwrap_or("primary"));
                state["showBirthdays"] = json!(payload["preferences"]["showBirthdays"]
                    .as_bool()
                    .unwrap_or(false));
                state["showLocation"] = json!(payload["preferences"]["showLocation"]
                    .as_bool()
                    .unwrap_or(false));
                state["openCommandJoinsMeeting"] = json!(payload["preferences"]
                    ["openCommandJoinsMeeting"]
                    .as_bool()
                    .unwrap_or(false));
            }
            if command.starts_with("create-") {
                let calendars = if command == "create-event" {
                    load_calendars(&api).await?
                } else {
                    vec![]
                };
                return Ok(create_form(
                    command,
                    &calendars,
                    &payload["preferences"],
                    state,
                ));
            }
            refresh(&api, command, state).await?;
            Ok(render(command, state))
        }
        "extension:search-text-changed" => {
            if command == "search-contacts" {
                state["query"] = json!(field(payload, "searchText"));
                refresh(&api, command, state).await?;
            }
            Ok(render(command, state))
        }
        "extension:load-more" => {
            if (command == "list-events" || state["calendarView"] == true)
                && !field(state, "nextPageToken").is_empty()
            {
                let cal = state["calendar"].as_str().unwrap_or("primary");
                let mut params = vec![
                    ("pageToken", field(state, "nextPageToken").into()),
                    ("maxResults", "50".into()),
                    ("singleEvents", "true".into()),
                    ("orderBy", "startTime".into()),
                    ("timeMin", field(state, "timeMin").into()),
                ];
                params.extend(event_types(cal, state["showBirthdays"] == true));
                let page = api.get(&cal_path(events_calendar(cal)), &params).await?;
                add_items(state, page["items"].as_array().cloned().unwrap_or_default());
                state["nextPageToken"] = page["nextPageToken"].clone();
            }
            Ok(render(command, state))
        }
        "extension:refresh-session" => {
            if command.starts_with("create-") {
                if state["createdEvent"].is_object() {
                    Ok(created_view(state))
                } else {
                    let calendars = if command == "create-event" {
                        load_calendars(&api).await?
                    } else {
                        vec![]
                    };
                    Ok(create_form(
                        command,
                        &calendars,
                        &payload["preferences"],
                        state,
                    ))
                }
            } else {
                refresh(
                    &api,
                    if state["calendarView"] == true {
                        "list-events"
                    } else {
                        command
                    },
                    state,
                )
                .await?;
                Ok(render(command, state))
            }
        }
        "extension:invoke-action" => invoke(ctx, &api, command, action_id, payload, state).await,
        _ => Err(format!(
            "Unsupported Google Calendar operation: {operation}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_and_date_guards() {
        assert_eq!(minutes("1h30m").unwrap(), 90);
        assert_eq!(minutes("45 min").unwrap(), 45);
        assert!(minutes("2h garbage").is_err());
        assert!(minutes("0").is_err());
        let prefs =
            json!({"addSignature":false,"defaultEventDuration":"30","sendInvitations":"none"});
        let (id,body,query)=event_body(&json!({"calendar":"primary","title":"Review","allDay":true,"startDate":"2026-12-31","endDate":"2027-01-01"}),&prefs).unwrap();
        assert_eq!(id, "primary");
        assert_eq!(body["end"]["date"], "2027-01-02");
        assert_eq!(query[0].1, "none");
        assert!(event_body(
            &json!({"title":"Meeting","startDate":"2027-01-01T10:00:00","attendees":"bad"}),
            &prefs
        )
        .is_err());
    }
    #[tokio::test]
    async fn fixture_records_bearer_pagination_and_event_write() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let recorder = tokio::spawn(async move {
            let mut seen = vec![];
            for step in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 8192];
                let size = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..size]).to_string();
                seen.push(request.clone());
                let body = match step {
                    0 => r#"{"items":[{"id":"first"}],"nextPageToken":"next-page"}"#,
                    1 => r#"{"items":[{"id":"second"}]}"#,
                    _ => r#"{"id":"created","summary":"Meeting"}"#,
                };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            seen
        });
        let http = client().unwrap();
        let api = Google {
            http: &http,
            base: &base,
            token: "fixture-token",
        };
        let items = api
            .pages("/calendar/v3/users/me/calendarList", &[])
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        let created = api
            .post(
                "/calendar/v3/calendars/primary/events",
                &[("sendUpdates", "none".into())],
                &json!({"summary":"Meeting"}),
            )
            .await
            .unwrap();
        assert_eq!(created["id"], "created");
        let seen = recorder.await.unwrap();
        assert!(seen.iter().all(
            |request| request.contains("Authorization: Bearer fixture-token")
                || request.contains("authorization: Bearer fixture-token")
        ));
        assert!(seen[1].contains("pageToken=next-page"));
        assert!(seen[2].contains("POST /calendar/v3/calendars/primary/events?sendUpdates=none"));
    }
    #[tokio::test]
    async fn fixture_refresh_request_sends_form_and_parses_token_without_real_grant() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let n = stream.read(&mut request).await.unwrap();
            let observed = String::from_utf8_lossy(&request[..n]).into_owned();
            let body = r#"{"access_token":"fixture-access","expires_in":3600}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
            observed
        });
        let result = token_request(
            &client().unwrap(),
            &endpoint,
            &[
                ("client_id", "fixture-client"),
                ("refresh_token", "fixture-refresh"),
                ("grant_type", "refresh_token"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(result["access_token"], "fixture-access");
        let observed = server.await.unwrap();
        assert!(observed.contains("POST /token"));
        assert!(observed.contains("application/x-www-form-urlencoded"));
    }
    #[tokio::test]
    async fn revoked_refresh_returns_connect_view_but_transient_errors_preserve_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(crate::native_core::common::CorePaths {
            data_dir: temp.path().into(),
            config_dir: temp.path().join("config"),
            resource_dir: temp.path().join("resources"),
            temp_dir: temp.path().join("tmp"),
            home_dir: temp.path().into(),
        });
        let expired = json!({"access_token":"fixture-expired","refresh_token":"fixture-revoked","expires_at":0});
        save_tokens(&ctx, &expired).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (status, body) in [
                (503, r#"{"error":"temporarily_unavailable"}"#),
                (
                    400,
                    r#"{"error":"invalid_grant","error_description":"Token revoked"}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 8192];
                let _ = socket.read(&mut request).await.unwrap();
                socket.write_all(format!("HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let http = client().unwrap();
        assert!(refresh_access_token(
            &ctx,
            &http,
            &endpoint,
            "fixture-desktop-client",
            expired.clone()
        )
        .await
        .is_err());
        assert_eq!(
            read_tokens(&ctx).unwrap(),
            expired,
            "a transient service error must not erase a valid grant"
        );
        assert!(
            refresh_access_token(&ctx, &http, &endpoint, "fixture-desktop-client", expired)
                .await
                .unwrap()
                .is_none()
        );
        server.await.unwrap();
        assert_eq!(
            read_tokens(&ctx).unwrap(),
            json!({}),
            "invalid_grant must remove both access and refresh tokens"
        );
        assert!(access_token(&ctx, &http, &endpoint)
            .await
            .unwrap()
            .is_none());
        let view = auth_view();
        assert_eq!(view["mode"], "view");
        assert_eq!(view["actions"][0]["id"], "google:connect");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(auth_file(&ctx))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn tokens_are_separate_from_sdk_store_and_private() {
        let temp = tempfile::tempdir().unwrap();
        let p = crate::native_core::common::CorePaths {
            data_dir: temp.path().into(),
            config_dir: temp.path().join("config"),
            resource_dir: temp.path().join("res"),
            temp_dir: temp.path().join("tmp"),
            home_dir: temp.path().into(),
        };
        let ctx = CoreContext::headless(p);
        save_tokens(&ctx, &json!({"refresh_token":"fixture-only"})).unwrap();
        assert_eq!(read_tokens(&ctx).unwrap()["refresh_token"], "fixture-only");
        assert!(!ctx
            .paths
            .data_dir
            .join("native-extensions-v2")
            .join(SLUG)
            .exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(auth_file(&ctx))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
