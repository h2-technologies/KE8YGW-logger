use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
};

use ham_server::{parse_query, split_target, ApiRequest, HostedServer, SurrealHostedConfig};

fn main() {
    let addr = env::var("HAM_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:9750".to_owned());
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind ham-server to {addr}: {error}");
            process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("ham-server runtime should start");
    let metadata_config = SurrealHostedConfig::from_env();
    let metadata_label = metadata_config.label();
    let server = match HostedServer::with_surreal_config(metadata_config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "failed to open ham-server SurrealDB metadata store at {metadata_label}: {error}"
            );
            process::exit(1);
        }
    };

    println!("ham-server listening on http://{addr}");
    println!("metadata store: {metadata_label}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_stream(&server, &runtime, &mut stream) {
                    eprintln!("failed to handle request: {error}");
                }
            }
            Err(error) => eprintln!("failed to accept request: {error}"),
        }
    }
}

fn handle_stream(
    server: &HostedServer,
    runtime: &tokio::runtime::Runtime,
    stream: &mut TcpStream,
) -> std::io::Result<()> {
    let request = {
        let mut reader = BufReader::new(&mut *stream);
        read_http_request(&mut reader)?
    };
    let response = runtime.block_on(server.handle(request));
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "OK",
    };
    let extra_headers = response
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        status_text,
        extra_headers,
        response.body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&response.body)?;
    Ok(())
}

fn read_http_request(reader: &mut BufReader<&mut TcpStream>) -> std::io::Result<ApiRequest> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    let (path, query) = split_target(&target);

    let mut content_length = 0usize;
    let mut headers = std::collections::HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(name, value);
        }
    }

    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(ApiRequest {
        method,
        path: path.to_owned(),
        query: parse_query(query),
        headers,
        body,
    })
}
