//! A Messenger tab owned by one chat window. Keep playback and drafts in the
//! same tab as the encrypted thread so audio stays in Messenger's player.
use crate::{Fb, tf};
use patois::t;
use serde_json::{Value, json};

const SCRIPT: &str = include_str!("chat_voice.js");
const ORIGIN: &str = "https://www.facebook.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read(u32),
    Pulse,
    Play,
    StopPlayback,
    Start,
    Stop,
    Discard,
    Send,
    Close,
}

pub struct Session {
    page: String,
    chat: String,
    open: bool,
    microphone: bool,
}

impl Session {
    pub fn new(id: u64, chat: String) -> Self {
        Self { page: format!("chat-{id}"), chat, open: false, microphone: false }
    }

    pub fn run(&mut self, fb: &Fb, action: Action, clip: &str) -> Result<String, String> {
        let response = match action {
            Action::Read(count) => {
                if !self.open && self.open(fb).is_err() {
                    // Name-only and legacy rows retain the ordinary read path.
                    let args = if count >= 1_000_000 {
                        crate::fb::read_all_args(&self.chat)
                    } else {
                        crate::fb::read_args(&self.chat, count)
                    };
                    return fb.run(&args.iter().map(String::as_str).collect::<Vec<_>>());
                }
                self.eval(fb, "read", json!(count), false)
            }
            Action::Pulse => self.eval(fb, "pulse", Value::Null, true),
            Action::Play => self.eval(fb, "play", json!(clip), true),
            Action::StopPlayback => self.eval(fb, "stop-playback", Value::Null, false),
            Action::Start => self.start(fb),
            Action::Stop => {
                let response = self.eval(fb, "stop", Value::Null, false);
                self.restore_mic(fb);
                response
            }
            Action::Discard => {
                let response = self.eval(fb, "discard", Value::Null, false);
                self.restore_mic(fb);
                response
            }
            Action::Send => self.eval(fb, "send", Value::Null, true),
            Action::Close => {
                self.close(fb);
                return Ok(String::new());
            }
        };
        if response.is_err() || response.as_ref().is_ok_and(|value| value["error"] == "thread") {
            self.close(fb);
        }
        response.map(|value| value.to_string())
    }

    fn open(&mut self, fb: &Fb) -> Result<(), String> {
        let resolved;
        let spec = if thread_path(&self.chat).is_some() {
            &self.chat
        } else {
            resolved = fb.run(&["resolve", &self.chat])?;
            resolved.trim()
        };
        let path = thread_path(spec)
            .ok_or_else(|| t("Messenger voice controls require a specific conversation."))?
            .to_owned();
        let result = (|| {
            fb.run(&["page", &self.page, "open", &format!("{ORIGIN}{path}")])?;
            fb.run(&["page", &self.page, "eval", SCRIPT])?;
            let opened = self.eval(fb, "open", json!(path), false)?;
            if let Some(error) = opened["error"].as_str() {
                return Err(message(error));
            }
            Ok(())
        })();
        self.open = result.is_ok();
        if result.is_err() {
            let _ = fb.run(&["page", &self.page, "close"]);
        }
        result
    }

    fn eval(&self, fb: &Fb, action: &str, argument: Value, gesture: bool) -> Result<Value, String> {
        let expression = format!("window.__blindfbChatVoice?.({}, {argument})", json!(action));
        let response = self.cdp(
            fb,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": gesture,
            }),
        )?;
        response["result"]["value"]
            .as_object()
            .map(|object| Value::Object(object.clone()))
            .ok_or_else(|| {
                t("Messenger voice controls disconnected. Refresh the chat to reconnect.")
            })
    }

    fn cdp(&self, fb: &Fb, method: &str, params: Value) -> Result<Value, String> {
        let raw = fb.run(&["page", &self.page, "cdp", method, &params.to_string()])?;
        serde_json::from_str(&raw).map_err(|_| {
            t("Messenger returned an unreadable response. Refresh the chat to reconnect.")
        })
    }

    fn start(&mut self, fb: &Fb) -> Result<Value, String> {
        let permission = self.eval(fb, "permission", Value::Null, false)?;
        if permission["error"].is_string() {
            return Ok(permission);
        }
        match permission["permission"].as_str() {
            Some("granted") => {}
            Some("prompt") => {
                self.set_mic(fb, "granted")?;
                self.microphone = true;
            }
            _ => return Ok(json!({"error": "microphone"})),
        }
        let response = self.eval(fb, "start", Value::Null, true);
        if response.is_err() || response.as_ref().is_ok_and(|value| value["error"].is_string()) {
            self.restore_mic(fb);
        }
        response
    }

    fn set_mic(&self, fb: &Fb, setting: &str) -> Result<(), String> {
        let params =
            json!({"permission": {"name": "microphone"}, "setting": setting, "origin": ORIGIN});
        match self.cdp(fb, "Browser.setPermission", params.clone()) {
            Ok(_) => Ok(()),
            // The tab can expire during a reconnect while our permission override is active.
            Err(error) if setting == "prompt" => {
                let endpoint = fb.browser_endpoint().map_err(|_| error)?;
                crate::browsers::cdp_call(&endpoint, "Browser.setPermission", &params)?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn restore_mic(&mut self, fb: &Fb) {
        if self.microphone && self.set_mic(fb, "prompt").is_ok() {
            self.microphone = false;
        }
    }

    fn close(&mut self, fb: &Fb) {
        self.restore_mic(fb);
        if self.open {
            let _ = fb.run(&["page", &self.page, "close"]);
            self.open = false;
        }
    }
}

fn thread_path(target: &str) -> Option<&str> {
    let path = target.trim_end_matches('/');
    let id =
        path.strip_prefix("/messages/t/").or_else(|| path.strip_prefix("/messages/e2ee/t/"))?;
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then_some(path)
}

pub fn message(code: &str) -> String {
    match code {
        "thread" => t("Messenger opened a different conversation. Refresh the chat to reconnect."),
        "locked" => t(
            "Messenger is locked. Close this window and use Chat, Unlock encrypted chats, then reopen the chat.",
        ),
        "play" => t("Messenger could not play that voice message."),
        "no-recorder" => t("Messenger's voice recorder is unavailable in this conversation."),
        "recording" => t("Stop or discard the recording before using this control."),
        "microphone" => t(
            "Messenger could not start recording. Check microphone access for your browser in system settings.",
        ),
        "send" => t(
            "Messenger did not confirm the voice message. Check the conversation before retrying.",
        ),
        _ => tf!("Messenger voice controls reported an error: {error}", error = code),
    }
}

#[cfg(test)]
mod tests {
    use super::thread_path;

    #[test]
    fn only_specific_messenger_threads_can_open_voice_controls() {
        assert_eq!(thread_path("/messages/t/123/"), Some("/messages/t/123"));
        assert_eq!(thread_path("/messages/e2ee/t/123"), Some("/messages/e2ee/t/123"));
        for target in
            ["/messages/t/", "/messages/t/123?x=1", "/messages/t/../123", "//elsewhere.test"]
        {
            assert_eq!(thread_path(target), None);
        }
    }
}
