//! Signing in and listing packs.

use crate::ui;
use anyhow::{Result, bail};
use beacon_core::{api::Credentials, hub::Hub, settings::Settings};
use std::time::Duration;

pub async fn login(hub_url: Option<String>, use_password: bool) -> Result<()> {
    let mut s = Settings::load();
    let hub_url = match hub_url {
        Some(h) => h,
        None => ui::input("Hub URL", Some(s.hub_url()))?,
    };
    s.hub = Some(hub_url.trim_end_matches('/').to_string());
    let hub = Hub::new(&s.hub_url(), None);
    let auth = if use_password {
        let username = ui::input("Username", s.username.clone())?;
        let password = ui::password("Password")?;
        hub.login(&Credentials { username, password }).await?
    } else {
        let start = hub.device_start().await?;
        println!();
        println!("  Approve this sign in from the BeaconSync app (Settings › Approve a device)");
        println!("  or open {}/device", hub.base);
        println!();
        println!("  Code: {}", ui::bold(&start.user_code));
        println!();
        ui::info("Waiting for approval...");
        let deadline = std::time::Instant::now() + Duration::from_secs(start.expires_in);
        loop {
            if std::time::Instant::now() > deadline {
                bail!("the code expired, run `beacon login` again");
            }
            if let Some(a) = hub.device_poll(&start.device_code).await? {
                break a;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    };
    s.token = Some(auth.token);
    s.username = Some(auth.user.username.clone());
    s.save()?;
    ui::ok(format!("Signed in as {} on {}", auth.user.username, s.hub_url()));
    Ok(())
}

pub fn logout() -> Result<()> {
    let mut s = Settings::load();
    s.token = None;
    s.save()?;
    ui::ok("Signed out");
    Ok(())
}

pub async fn whoami() -> Result<()> {
    let s = Settings::load();
    let me = Hub::authed(&s)?.me().await?;
    println!("{} on {}", me.username, s.hub_url());
    Ok(())
}

pub async fn packs() -> Result<()> {
    let s = Settings::load();
    let packs = Hub::authed(&s)?.library().await?;
    if packs.is_empty() {
        ui::info("No packs yet. Publish one from the app or with `beacon publish`.");
    }
    for p in packs {
        let latest =
            p.latest.map(|r| format!("{} {}", r.name, ui::dim(ui::ago(r.created)))).unwrap_or_else(|| ui::dim("no releases"));
        let owner = if p.mine { String::new() } else { ui::dim(format!(" by {}", p.owner)) };
        println!("{}  {}{}  {} {}  {}", ui::bold(&p.code), p.name, owner, p.loader.kind.label(), p.minecraft, latest);
    }
    Ok(())
}
