use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

fn mock(status: u16, body: String, delay: Duration) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("mock accept: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = String::new();
        let mut len = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = value.trim().parse().unwrap();
            }
            headers.push_str(&line);
        }
        let mut request = vec![0; len];
        reader.read_exact(&mut request).unwrap();
        thread::sleep(delay);
        let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://127.0.0.1:1/\r\n\r\n{body}", body.len());
        // 超时测试中客户端会主动在响应前关闭连接。
        let _ = stream.write_all(response.as_bytes());
        format!("{headers}\r\n{}", String::from_utf8(request).unwrap())
    });
    (endpoint, handle)
}

#[test]
fn config_normalization_and_validation() {
    for (path, db) in [
        ("", "neo4j"),
        ("/", "neo4j"),
        ("/db/test", "test"),
        ("/db/test/query/v2/", "test"),
    ] {
        let store = Neo4jGraphStore::new(Neo4jGraphConfig::basic(
            format!("http://localhost:7474{path}"),
            "u",
            "p",
        ))
        .unwrap();
        assert_eq!(
            store.query_url,
            format!("http://localhost:7474/db/{db}/query/v2")
        );
    }
    let store = Neo4jGraphStore::new(
        Neo4jGraphConfig::basic("https://host/db/old/query/v2", "u", "p").with_database("new"),
    )
    .unwrap();
    assert_eq!(store.query_url, "https://host/db/new/query/v2");
    for endpoint in [
        "bolt://host",
        "http://u:p@host",
        "http://host/?secret=x",
        "http://host/#x",
        "http://host/db/a/tx/commit",
        "http://host/arbitrary",
        "http://host/db/a%2Fb",
    ] {
        assert!(Neo4jGraphStore::new(Neo4jGraphConfig::basic(endpoint, "u", "p")).is_err());
    }
    for db in ["", "..", "a/b", "a?x=1"] {
        assert!(Neo4jGraphStore::new(
            Neo4jGraphConfig::basic("http://host", "u", "p").with_database(db)
        )
        .is_err());
    }
    assert!(Neo4jGraphStore::new(
        Neo4jGraphConfig::basic("http://host", "u", "p").with_namespace(" ")
    )
    .is_err());
    assert!(Neo4jGraphStore::new(
        Neo4jGraphConfig::basic("http://host", "u", "p").with_timeout(Duration::ZERO)
    )
    .is_err());
}

#[test]
fn credentials_are_redacted() {
    for config in [
        Neo4jGraphConfig::basic("http://host", "private-user", "secret-password"),
        Neo4jGraphConfig::bearer("http://host", "secret-token"),
    ] {
        let debug = format!(
            "{config:?} {:?}",
            Neo4jGraphStore::new(config.clone()).unwrap()
        );
        for secret in ["private-user", "secret-password", "secret-token"] {
            assert!(!debug.contains(secret));
        }
    }
    assert_eq!(
        Neo4jAuth::Basic {
            username: "u".into(),
            password: "p".into()
        }
        .header(),
        "Basic dTpw"
    );
}

#[test]
fn query_uses_parameters_and_namespace() {
    let body = json!({"data":{"fields":["id"],"values":[[42]]}}).to_string();
    let (endpoint, handle) = mock(202, body, Duration::ZERO);
    let store = Neo4jGraphStore::new(
        Neo4jGraphConfig::basic(endpoint, "u", "p").with_namespace("tenant-甲"),
    )
    .unwrap();
    let subject = "Alice' MATCH (n) DETACH DELETE n //";
    assert_eq!(
        store
            .replace_triple(&Triple::new(subject, "住在", "上海"), Some(7), 12)
            .unwrap(),
        42
    );
    let request = handle.join().unwrap();
    assert!(request.starts_with("POST /db/neo4j/query/v2 HTTP/1.1"));
    assert!(request.contains("Authorization: Basic dTpw"));
    let payload: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(!payload["statement"].as_str().unwrap().contains(subject));
    assert_eq!(payload["parameters"]["subject"], subject);
    assert_eq!(payload["parameters"]["namespace"], "tenant-甲");
    assert_eq!(payload["parameters"]["replace"], true);
}

#[test]
fn server_errors_and_malformed_rows_are_not_successes() {
    for payload in [
        json!({}),
        json!({"errors":"bad"}),
        json!({"data":{"fields":[],"values":[[1]]}}),
        json!({"data":{"fields":["x"],"values":[{}]}}),
        json!({"errors":[{"code":"Neo.ClientError.Statement.SyntaxError", "message":"secret-password"}]}),
    ] {
        let error = Neo4jGraphStore::parse_response(&payload)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("secret-password"));
    }
    let (endpoint, handle) = mock(
        202,
        json!({"errors":[{"code":"Neo.ClientError.Statement.SyntaxError"}]}).to_string(),
        Duration::ZERO,
    );
    let store = Neo4jGraphStore::new(Neo4jGraphConfig::basic(endpoint, "u", "p")).unwrap();
    assert!(store
        .entities()
        .unwrap_err()
        .to_string()
        .contains("SyntaxError"));
    handle.join().unwrap();
    assert!(integer(&json!(u64::MAX)).is_err());
    assert!(parse_edge(&json!([1, "s", "p", "o", null, 1.1, 0, null])).is_err());
}

#[test]
fn transport_enforces_status_size_and_timeout() {
    for (status, body, limit, delay, timeout) in [
        (401, "secret-password", 1024, 0, 2000),
        (302, "", 1024, 0, 2000),
        (202, "not json", 1024, 0, 2000),
        (202, "123456789", 4, 0, 2000),
        (202, "{}", 1024, 200, 50),
    ] {
        let (endpoint, handle) = mock(status, body.into(), Duration::from_millis(delay));
        let mut config = Neo4jGraphConfig::basic(endpoint, "u", "p")
            .with_timeout(Duration::from_millis(timeout));
        config.max_response_bytes = limit;
        let store = Neo4jGraphStore::new(config).unwrap();
        let error = store.entities().unwrap_err().to_string();
        assert!(!error.contains("secret-password"));
        handle.join().unwrap();
    }
}

#[test]
fn invalid_input_and_zero_depth_do_not_contact_server() {
    let store =
        Neo4jGraphStore::new(Neo4jGraphConfig::basic("http://127.0.0.1:1", "u", "p")).unwrap();
    assert!(store.neighbors("Alice", 0).unwrap().is_empty());
    assert!(store.find_paths("Alice", "Bob", 0).unwrap().is_empty());
    for triple in [
        Triple::new(" ", "p", "o"),
        Triple::new("s", "p", "o").with_confidence(f32::NAN),
    ] {
        assert!(matches!(
            store.add_triple(&triple, None, 1),
            Err(StoreError::Other(_))
        ));
    }
}
