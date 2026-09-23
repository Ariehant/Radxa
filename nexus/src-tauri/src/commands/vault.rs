use crate::rpc::Router;
use crate::vault::Vault;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct PathParams {
    path: String,
}

pub fn register(r: &mut Router) {
    r.add("vault.open", |s, p: PathParams| {
        let v = Vault::open(&PathBuf::from(&p.path), s.events.clone())?;
        let info = v.info();
        s.set_vault(Some(v));
        Ok(info)
    });
    r.add("vault.create", |s, p: PathParams| {
        let v = Vault::create(&PathBuf::from(&p.path), s.events.clone())?;
        let info = v.info();
        s.set_vault(Some(v));
        Ok(info)
    });
    r.add("vault.close", |s, _p: serde_json::Value| {
        s.set_vault(None);
        Ok(())
    });
    r.add("vault.info", |s, _p: serde_json::Value| Ok(s.vault().ok().map(|v| v.info())));
    r.add("vault.tree", |s, _p: serde_json::Value| s.vault()?.tree());
}
