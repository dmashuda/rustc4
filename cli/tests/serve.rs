use super::*;
use std::io::Read;
use std::net::Ipv4Addr;

const LOCAL: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// The whole answer to a GET, headers and all.
fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn serves_the_site_and_follows_the_files() {
    let dir = crate::tests::scratch("serve");
    let file = dir.join("m.c4");
    let model = |name: &str| {
        format!(
            "specification {{ element box }} model {{ a = box '{name}' }} views {{ view index {{ include * }} }}"
        )
    };
    std::fs::write(&file, model("Before")).unwrap();
    let server = Server::bind(vec![dir.clone()], LOCAL, 0, true).unwrap();
    let addr = server.addr().unwrap();
    std::thread::spawn(move || server.run(&mut std::io::sink(), &mut std::io::sink()));

    let page = get(addr, "/");
    assert!(page.starts_with("HTTP/1.1 200 OK\r\ncontent-type: text/html"), "{page}");
    assert!(page.contains("<script"));
    assert!(get(addr, "/rustc4_bg.wasm").contains("content-type: application/wasm\r\n"));
    let json = get(addr, "/model.json?now");
    assert!(json.contains("x-rustc4-changes: changes\r\n") && json.contains("Before"), "{json}");
    assert!(get(addr, "/views/index.svg").contains(">Before</text>"));
    assert!(get(addr, "/views/nope.svg").starts_with("HTTP/1.1 404 "));
    assert!(get(addr, "/../m.c4").starts_with("HTTP/1.1 404 "));

    // A page listening hears at once, and again when the file is saved.
    let mut events = TcpStream::connect(addr).unwrap();
    write!(events, "GET /changes HTTP/1.1\r\n\r\n").unwrap();
    let mut events = BufReader::new(events);
    let mut next = || loop {
        let mut line = String::new();
        assert!(events.read_line(&mut line).unwrap() > 0, "the stream ended");
        if let Some(data) = line.strip_prefix("data: ") {
            return data.trim().to_string();
        }
    };
    assert_eq!(next(), "0");
    std::fs::write(&file, model("Afterwards")).unwrap();
    assert_eq!(next(), "1");
    assert!(get(addr, "/model.json").contains("Afterwards"));
    assert!(get(addr, "/views/index.svg").contains(">Afterwards</text>"));

    // One that cannot be read leaves the last one up; a new file joins.
    std::fs::remove_file(&file).unwrap();
    std::fs::write(dir.join("n.c4"), model("Another file")).unwrap();
    assert_eq!(next(), "2");
    assert!(get(addr, "/model.json").contains("n.c4"));
}

#[test]
fn says_when_the_port_is_taken() {
    let dir = crate::tests::scratch("serve-port");
    std::fs::write(dir.join("m.c4"), "model {}").unwrap();
    let first = Server::bind(vec![dir.clone()], LOCAL, 0, true).unwrap();
    let port = first.addr().unwrap().port();
    let message = Server::bind(vec![dir], LOCAL, port, true).err().unwrap();
    assert!(message.starts_with(&format!("port {port}: ")) && message.ends_with("--port)"), "{message}");
}

/// Listening to everyone, as in a container, is still reached from here.
#[test]
fn listens_where_it_is_told() {
    let dir = crate::tests::scratch("serve-host");
    std::fs::write(dir.join("m.c4"), "model {}").unwrap();
    let server = Server::bind(vec![dir], IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0, true).unwrap();
    let addr = server.addr().unwrap();
    assert!(addr.ip().is_unspecified());
    std::thread::spawn(move || server.run(&mut std::io::sink(), &mut std::io::sink()));
    let page = get(SocketAddr::new(LOCAL, addr.port()), "/");
    assert!(page.starts_with("HTTP/1.1 200 OK\r\n"), "{page}");
}
