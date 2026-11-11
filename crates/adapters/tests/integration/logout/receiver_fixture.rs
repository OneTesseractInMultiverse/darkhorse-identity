use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

pub(super) struct Receiver {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    pub origin: String,
    pub ca: String,
    pub jwks_url: String,
    refresh_cooldown_ms: u64,
}
impl Receiver {
    pub fn start() -> Self {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/logout-receiver-fixture.mjs"
        );
        let mut child = Command::new(std::env::var("NODE").unwrap_or_else(|_| "node".into()))
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("reference receiver needs the pinned Node and OpenSSL tools");
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut receiver = Self {
            child,
            input,
            output,
            origin: String::new(),
            ca: String::new(),
            jwks_url: String::new(),
            refresh_cooldown_ms: 0,
        };
        let ready = receiver.read();
        receiver.origin = ready["origin"].as_str().unwrap().into();
        receiver.ca = ready["ca"].as_str().unwrap().into();
        receiver.jwks_url = ready["jwksUrl"].as_str().unwrap().into();
        receiver.refresh_cooldown_ms = ready["refreshCooldownMs"].as_u64().unwrap();
        receiver
    }
    pub fn configure(&mut self, issuer: &str, client: u128) {
        self.command(json!({"command":"configure", "path":format!("/{client}"), "issuer":issuer,
            "client":uuid::Uuid::from_u128(client).to_string(), "jwksUrl":self.jwks_url,"ca":self.ca}));
    }
    pub async fn refresh_window(&self) {
        tokio::time::sleep(std::time::Duration::from_millis(
            self.refresh_cooldown_ms + 100,
        ))
        .await;
    }
    fn read(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "reference receiver exited early"
        );
        serde_json::from_str(&line).unwrap()
    }
    pub fn command(&mut self, value: Value) -> Value {
        writeln!(self.input.as_mut().unwrap(), "{value}").unwrap();
        let result = self.read();
        assert!(
            result.get("error").is_none(),
            "reference receiver rejected fixture control"
        );
        result["result"].clone()
    }
    pub fn sessions(&mut self, expected: &[&str]) {
        assert_eq!(self.command(json!({"command":"sessions"})), json!(expected));
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.input.take();
        let result = self.child.wait();
        if !std::thread::panicking() {
            assert!(
                result.is_ok_and(|status| status.success()),
                "reference receiver cleanup failed"
            );
        }
    }
}
