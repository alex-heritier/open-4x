//! Both embedded and remote transports accept the exact same request/response protocol.
use fourx_content::Pack;
use fourx_runtime::Host;
use fourx_sim::{Request, Response};
use std::collections::VecDeque;

pub enum Event {
    Response(Response),
    Content(Pack),
    Error(String),
}
fn decode(text: &str) -> Event {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) if v["type"] == "content" => {
            match serde_json::from_value::<Pack>(v["pack"].clone()) {
                Ok(p) if p.validate().is_ok() => Event::Content(p),
                _ => Event::Error("Invalid server content manifest".into()),
            }
        }
        _ => match serde_json::from_str(text) {
            Ok(r) => Event::Response(r),
            Err(e) => Event::Error(format!("Invalid server response: {e}")),
        },
    }
}
pub struct Connection {
    host: Option<Host>,
    inbox: VecDeque<Event>,
    #[cfg(not(target_arch = "wasm32"))]
    remote: Option<NativeRemote>,
    #[cfg(target_arch = "wasm32")]
    remote: Option<WebRemote>,
}
impl Connection {
    pub fn local(host: Host) -> Self {
        let inbox = VecDeque::from([
            Event::Content(host.pack.clone()),
            Event::Response(host.snapshot(1)),
        ]);
        Self {
            host: Some(host),
            inbox,
            remote: None,
        }
    }
    pub fn remote(url: String, token: String) -> Self {
        let mut connection = Self {
            host: None,
            inbox: VecDeque::new(),
            remote: None,
        };
        match connect(url, token) {
            Ok(remote) => connection.remote = Some(remote),
            Err(e) => connection.inbox.push_back(Event::Error(e)),
        }
        connection
    }
    pub fn send(&mut self, request: Request) {
        if let Some(host) = &mut self.host {
            self.inbox
                .push_back(Event::Response(host.request(1, request)));
        } else if let Some(remote) = &mut self.remote {
            if let Err(e) = remote.send(serde_json::to_string(&request).unwrap()) {
                self.inbox.push_back(Event::Error(e));
            }
        }
    }
    pub fn poll(&mut self) -> Vec<Event> {
        if let Some(remote) = &mut self.remote {
            self.inbox.extend(remote.poll());
        }
        self.inbox.drain(..).collect()
    }
    pub fn save(&self) -> Result<String, String> {
        self.host
            .as_ref()
            .ok_or("Save from the authoritative server when playing remotely".into())
            .and_then(|h| h.save().map_err(|e| e.to_string()))
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct NativeRemote {
    tx: std::sync::mpsc::Sender<String>,
    rx: std::sync::mpsc::Receiver<Event>,
}
#[cfg(not(target_arch = "wasm32"))]
fn connect(url: String, token: String) -> Result<NativeRemote, String> {
    let (tx, commands) = std::sync::mpsc::channel::<String>();
    let (events, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use tungstenite::{Message, stream::MaybeTlsStream};
        let result = (|| -> Result<(), String> {
            let (mut socket, _) = tungstenite::connect(&url).map_err(|e| e.to_string())?;
            match socket.get_mut() {
                MaybeTlsStream::Plain(s) => s
                    .set_read_timeout(Some(std::time::Duration::from_millis(20)))
                    .map_err(|e| e.to_string())?,
                MaybeTlsStream::Rustls(s) => s
                    .sock
                    .set_read_timeout(Some(std::time::Duration::from_millis(20)))
                    .map_err(|e| e.to_string())?,
                _ => return Err("Unsupported transport".into()),
            }
            socket
                .send(Message::Text(
                    serde_json::json!({"type":"join","token":token}).to_string(),
                ))
                .map_err(|e| e.to_string())?;
            loop {
                loop {
                    match commands.try_recv() {
                        Ok(text) => socket
                            .send(Message::Text(text))
                            .map_err(|e| e.to_string())?,
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
                    }
                }
                match socket.read() {
                    Ok(Message::Text(t)) => {
                        if events.send(decode(&t)).is_err() {
                            return Ok(());
                        }
                    }
                    Ok(Message::Close(_)) => return Err("Server disconnected".into()),
                    Err(tungstenite::Error::Io(e))
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(e) => return Err(e.to_string()),
                    _ => {}
                }
            }
        })();
        if let Err(e) = result {
            let _ = events.send(Event::Error(e));
        }
    });
    Ok(NativeRemote { tx, rx })
}
#[cfg(not(target_arch = "wasm32"))]
impl NativeRemote {
    fn send(&mut self, text: String) -> Result<(), String> {
        self.tx.send(text).map_err(|_| "Server disconnected".into())
    }
    fn poll(&mut self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}

#[cfg(target_arch = "wasm32")]
struct WebRemote {
    socket: web_sys::WebSocket,
    inbox: std::rc::Rc<std::cell::RefCell<VecDeque<Event>>>,
    _open: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    _message: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    _close: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}
#[cfg(target_arch = "wasm32")]
fn connect(url: String, token: String) -> Result<WebRemote, String> {
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen::{JsCast, closure::Closure};
    let socket = web_sys::WebSocket::new(&url).map_err(|e| format!("{e:?}"))?;
    let inbox = Rc::new(RefCell::new(VecDeque::new()));
    let sock = socket.clone();
    let errors = inbox.clone();
    let open = Closure::wrap(Box::new(move |_: web_sys::Event| {
        if sock
            .send_with_str(&serde_json::json!({"type":"join","token":token}).to_string())
            .is_err()
        {
            errors
                .borrow_mut()
                .push_back(Event::Error("Join failed".into()));
        }
    }) as Box<dyn FnMut(_)>);
    socket.set_onopen(Some(open.as_ref().unchecked_ref()));
    let incoming = inbox.clone();
    let message = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            incoming.borrow_mut().push_back(decode(&text));
        }
    }) as Box<dyn FnMut(_)>);
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    let errors = inbox.clone();
    let error = Closure::wrap(Box::new(move |_: web_sys::Event| {
        errors.borrow_mut().push_back(Event::Error(
            "WebSocket failed. Check server URL and TLS.".into(),
        ));
    }) as Box<dyn FnMut(_)>);
    socket.set_onerror(Some(error.as_ref().unchecked_ref()));
    let errors = inbox.clone();
    let close = Closure::wrap(Box::new(move |_: web_sys::Event| {
        errors
            .borrow_mut()
            .push_back(Event::Error("Server disconnected".into()));
    }) as Box<dyn FnMut(_)>);
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    Ok(WebRemote {
        socket,
        inbox,
        _open: open,
        _message: message,
        _error: error,
        _close: close,
    })
}
#[cfg(target_arch = "wasm32")]
impl WebRemote {
    fn send(&mut self, text: String) -> Result<(), String> {
        self.socket
            .send_with_str(&text)
            .map_err(|_| "Server is not connected".into())
    }
    fn poll(&mut self) -> Vec<Event> {
        self.inbox.borrow_mut().drain(..).collect()
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for WebRemote {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}
