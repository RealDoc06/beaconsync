//! Tiny server rendered pages: landing, share links (`/j/CODE`) and device approval for
//! signing in the CLI from a browser.

use crate::routes::{AppState, approve_device, check_credentials, public_info};
use axum::{
    Form,
    extract::{Path, State},
    response::Html,
};
use beacon_core::api::Credentials;
use serde::Deserialize;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title><style>
:root{{color-scheme:dark}}*{{box-sizing:border-box}}
body{{margin:0;background:#000;color:#fff;font:15px/1.5 ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif;display:flex;min-height:100vh;align-items:center;justify-content:center;padding:16px}}
main{{width:100%;max-width:420px}}h1{{font-size:20px;margin:0 0 12px}}p{{color:#bbb;margin:8px 0}}
code{{font:600 28px/1.2 ui-monospace,monospace;letter-spacing:3px;display:block;margin:16px 0;color:#fff}}
input,button{{width:100%;font:inherit;padding:10px 12px;margin:6px 0;border-radius:6px;border:1px solid #333;background:#0a0a0a;color:#fff}}
button{{background:#fff;color:#000;border:0;font-weight:600;cursor:pointer}}.err{{color:#ff6b6b}}.ok{{color:#5ee08a}}a{{color:#fff}}
</style></head><body><main>{body}</main></body></html>"#
    ))
}

pub async fn home(State(s): State<AppState>) -> Html<String> {
    page(
        "BeaconSync hub",
        &format!(
            "<h1>BeaconSync hub</h1><p>This hub stores modpacks and deploys them to servers.</p><p>Point the BeaconSync app or CLI at <b>{}</b>.</p><p><a href=\"/device\">Approve a CLI sign in</a></p>",
            esc(&s.public_url())
        ),
    )
}

pub async fn share(State(s): State<AppState>, Path(code): Path<String>) -> Html<String> {
    match public_info(&s, &code) {
        Ok(p) => page(
            &format!("{} on BeaconSync", esc(&p.name)),
            &format!(
                "<h1>{}</h1><p>by {} · Minecraft {} · {}</p><p>Open BeaconSync, choose <b>Join with code</b> and enter:</p><code>{}</code><p>or paste this page's link.</p>",
                esc(&p.name),
                esc(&p.owner),
                esc(&p.minecraft),
                p.loader.kind.label(),
                esc(&p.code)
            ),
        ),
        Err(_) => page("Not found", "<h1>Pack not found</h1><p>Check the code and try again.</p>"),
    }
}

fn device_html(msg: &str) -> Html<String> {
    page(
        "Approve sign in",
        &format!(
            r#"<h1>Approve a sign in</h1><p>Enter the code shown by <b>beacon login</b> and your BeaconSync account.</p>{msg}
<form method="post"><input name="user_code" placeholder="ABCD-EFGH" autocomplete="off" required>
<input name="username" placeholder="Username" autocomplete="username" required>
<input name="password" type="password" placeholder="Password" autocomplete="current-password" required>
<button>Approve</button></form>"#
        ),
    )
}

pub async fn device_form() -> Html<String> {
    device_html("")
}

#[derive(Deserialize)]
pub struct DeviceForm {
    user_code: String,
    username: String,
    password: String,
}

pub async fn device_submit(State(s): State<AppState>, Form(f): Form<DeviceForm>) -> Html<String> {
    let result = check_credentials(&s, &Credentials { username: f.username, password: f.password })
        .and_then(|u| approve_device(&s, &u.id, &f.user_code));
    match result {
        Ok(()) => page("Approved", "<h1 class=\"ok\">Approved</h1><p>You can close this page and go back to the terminal.</p>"),
        Err(e) => device_html(&format!("<p class=\"err\">{}</p>", esc(&e.1))),
    }
}
