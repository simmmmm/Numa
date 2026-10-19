use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub mod sync;

pub const PORT: u16 = 8283;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    pub name: String,

    #[serde(default)]
    pub token: String,
}

impl Account {
    pub fn signed_in(&self) -> bool {
        !self.token.is_empty()
    }

    pub fn header(&self) -> String {
        format!("Authorization: Bearer {}\n", self.token)
    }
}

pub fn base(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.contains("://") {
        return address.to_string();
    }
    let host = address.split('/').next().unwrap_or_default();
    let has_port = host.rsplit_once(':').is_some_and(|(_, port)| port.parse::<u16>().is_ok());
    match has_port {
        true => format!("http://{address}"),
        false => format!("http://{address}:{PORT}"),
    }
}

pub fn host(address: &str) -> String {
    let address = address.trim();
    let address = address.split_once("://").map_or(address, |(_, rest)| rest);
    address.split('/').next().unwrap_or_default().to_string()
}

pub fn same_server(a: &str, b: &str) -> bool {
    let name = |address: &str| {
        let host = host(address).to_ascii_lowercase();
        match host.rsplit_once(':') {
            Some((name, port)) if port.parse::<u16>().is_ok() => name.to_string(),
            _ => host,
        }
    };
    name(a) == name(b)
}

pub fn open_link(uri: &str) -> Option<(String, String)> {
    let query = uri.strip_prefix("numa://home/open?")?;
    let (mut server, mut photo) = (None, None);
    for pair in query.split('&') {
        match pair.split_once('=')? {
            ("server", value) => server = Some(unescaped(value)?),
            ("photo", value) => photo = Some(value.to_string()),
            _ => {}
        }
    }
    let (server, photo) = (server?, photo?);
    let web = server.starts_with("http://") || server.starts_with("https://");
    let id = !photo.is_empty() && photo.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    (web && id).then_some((server, photo))
}

fn unescaped(text: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        if byte != b'%' {
            bytes.push(byte);
            continue;
        }
        let hex = std::str::from_utf8(rest.get(..2)?).ok()?;
        bytes.push(u8::from_str_radix(hex, 16).ok()?);
        rest = &rest[2..];
    }
    String::from_utf8(bytes).ok()
}

fn curl(rest: impl IntoIterator<Item = OsString>, url: String) -> Vec<OsString> {
    let start = ["curl", "--silent", "--show-error", "--connect-timeout", "10"].map(OsString::from);
    start.into_iter().chain(rest).chain(["--write-out".into(), "\n%{http_code}".into(), url.into()]).collect()
}

fn words(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

pub fn login(address: &str) -> Vec<OsString> {
    let rest = ["--max-time", "30", "--header", "Content-Type: application/json", "--data-binary", "@-"];
    curl(words(&rest), format!("{}/api/v1/login", base(address)))
}

pub fn login_body(name: &str, password: &str, device: &str) -> String {
    serde_json::json!({ "name": name, "password": password, "device": device }).to_string()
}

pub fn logout(address: &str) -> Vec<OsString> {
    curl(words(&["--max-time", "10", "--request", "POST", "--header", "@-"]), format!("{}/api/v1/logout", base(address)))
}

pub fn send(address: &str, file: &Path, sha256: &str) -> Vec<OsString> {
    send_to(address, file, sha256, "")
}

pub fn send_into(address: &str, file: &Path, sha256: &str, library: &str, dir: &str) -> Vec<OsString> {
    send_to(address, file, sha256, &format!("?library={}&dir={}", escaped(library), escaped(dir)))
}

fn send_to(address: &str, file: &Path, sha256: &str, query: &str) -> Vec<OsString> {
    let name = file.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let sha256 = format!("X-Numa-Sha256: {sha256}");
    let rest = words(&["--header", "@-", "--header", &sha256, "--upload-file"]).into_iter().chain([file.into()]);
    curl(rest, format!("{}/api/v1/send/{}{query}", base(address), escaped(&name)))
}

pub fn edits(address: &str, id: &str, file: &Path) -> Vec<OsString> {
    let rest = words(&["--header", "@-", "--header", "Content-Type: application/json", "--upload-file"]).into_iter().chain([file.into()]);
    curl(rest, format!("{}/api/v1/photos/{}/edits", base(address), escaped(id)))
}

fn escaped(name: &str) -> String {
    name.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[derive(Debug, PartialEq)]
pub enum Failure {

    SignIn(String),

    Unreachable,

    Said(String),
}

#[derive(Deserialize)]
struct Error {
    error: String,
}

fn answer(out: &str) -> Result<&str, Failure> {
    let (body, status) = out.rsplit_once('\n').unwrap_or(("", out));
    let status: u16 = status.trim().parse().unwrap_or(0);
    let sentence = || serde_json::from_str::<Error>(body).map(|said| said.error);
    match status {
        0 => Err(Failure::Unreachable),
        200..=299 => Ok(body),
        401 => Err(Failure::SignIn(sentence().unwrap_or_else(|_| "Sign in again".to_string()))),
        _ => Err(Failure::Said(sentence().unwrap_or_else(|_| format!("The server answered {status}")))),
    }
}

fn parsed<'a, T: Deserialize<'a>>(body: &'a str) -> Result<T, Failure> {
    serde_json::from_str(body).map_err(|_| Failure::Said("The server's answer could not be read".to_string()))
}

#[derive(Deserialize)]
struct Login {
    token: String,
    name: String,
}

pub fn signed_in(out: &str) -> Result<(String, String), Failure> {
    let login: Login = parsed(answer(out)?)?;
    Ok((login.token, login.name))
}

#[derive(Debug, PartialEq)]
pub struct Sent {
    pub id: String,
    pub duplicate: bool,
}

pub fn sent(out: &str) -> Result<Sent, Failure> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(deserialize_with = "id_text")]
        id: String,
        #[serde(default)]
        duplicate: bool,
    }
    let answer: Answer = parsed(answer(out)?)?;
    Ok(Sent { id: answer.id, duplicate: answer.duplicate })
}

fn id_text<'de, D: serde::Deserializer<'de>>(from: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(from)? {
        serde_json::Value::String(id) => id,
        other => other.to_string(),
    })
}

pub fn taken(out: &str) -> Result<(), Failure> {
    answer(out).map(drop)
}

pub fn originals(paths: &[PathBuf]) -> Vec<usize> {
    let key = |path: &Path| (path.parent().map(Path::to_path_buf), path.file_stem().map(|stem| stem.to_string_lossy().to_lowercase()));
    let raws: HashSet<_> = paths.iter().filter(|path| crate::raw::is_raw(path)).map(|path| key(path)).collect();
    (0..paths.len()).filter(|&at| crate::raw::is_raw(&paths[at]) || !raws.contains(&key(&paths[at]))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_without_a_port_gets_homes() {
        assert_eq!(base("nas.local"), "http://nas.local:8283");
        assert_eq!(base(" 192.168.1.20:9000/ "), "http://192.168.1.20:9000");
        assert_eq!(base("https://photos.example.org/"), "https://photos.example.org");
        assert_eq!(base("[::1]"), "http://[::1]:8283");
        assert_eq!(host("http://nas.local:8283/"), "nas.local:8283");
        assert_eq!(host("nas.local"), "nas.local");
        assert!(same_server("https://Numa.example.org", "numa.example.org"));
        assert!(same_server("http://[::1]:8297", "[::1]"));
        assert!(!same_server("https://photos.example.org", "nas.local"));
    }

    #[test]
    fn an_open_in_numa_link_is_read_or_refused() {
        let link = |uri| open_link(uri);
        assert_eq!(
            link("numa://home/open?server=http%3A%2F%2Flocalhost%3A8297&photo=4294967301"),
            Some(("http://localhost:8297".into(), "4294967301".into()))
        );
        assert_eq!(link("numa://home/open?photo=7&server=https%3A%2F%2Fphotos.example.org"), Some(("https://photos.example.org".into(), "7".into())));
        assert_eq!(link("numa://home/open?server=http%3A%2F%2Fnas&photo=../7"), None);
        assert_eq!(link("numa://home/open?server=file%3A%2F%2F%2Fetc&photo=7"), None);
        assert_eq!(link("numa://home/open?server=http%3A%2F%2Fnas%2&photo=7"), None);
        assert_eq!(link("numa://home/open?photo=7"), None);
        assert_eq!(link("numa://elsewhere?server=http%3A%2F%2Fnas&photo=7"), None);
    }

    #[test]
    fn the_token_and_the_password_never_go_on_the_command_line() {
        let account = Account { address: "nas".into(), name: "Amelie".into(), token: "secret-token".into() };
        let file = Path::new("/tmp/My photo #1.RAF");
        for argv in [send(&account.address, file, "ab12"), edits(&account.address, "7", file), logout("nas"), login("nas")] {
            assert!(!argv.iter().any(|arg| arg.to_string_lossy().contains("secret")), "{argv:?}");
            assert_eq!(argv[0], "curl");
        }
        assert_eq!(account.header(), "Authorization: Bearer secret-token\n");
        assert!(login_body("Amelie", "pw", "desk").contains("\"password\":\"pw\""));
    }

    #[test]
    fn send_puts_the_file_with_its_hash() {
        let argv = send("nas", Path::new("/tmp/My photo #1.RAF"), "ab12");
        let text: Vec<String> = argv.iter().map(|arg| arg.to_string_lossy().to_string()).collect();
        let at = text.iter().position(|arg| arg == "--upload-file").unwrap();
        assert_eq!(text[at + 1], "/tmp/My photo #1.RAF");
        assert!(text.contains(&"X-Numa-Sha256: ab12".to_string()));
        assert!(text.windows(2).any(|pair| pair == ["--header", "@-"]));
        assert_eq!(text.last().unwrap(), "http://nas:8283/api/v1/send/My%20photo%20%231.RAF");
        let edits = edits("nas", "7", Path::new("/tmp/e.json"));
        assert_eq!(edits.last().unwrap(), "http://nas:8283/api/v1/photos/7/edits");
    }

    #[test]
    fn the_answers_are_read() {
        assert_eq!(sent("{\"id\":7,\"duplicate\":false}\n201"), Ok(Sent { id: "7".into(), duplicate: false }));
        assert_eq!(sent("{\"id\":\"a1\",\"duplicate\":true}\n200"), Ok(Sent { id: "a1".into(), duplicate: true }));
        assert_eq!(signed_in("{\"token\":\"t\",\"name\":\"Fred\",\"admin\":false}\n200"), Ok(("t".into(), "Fred".into())));
        assert_eq!(sent("\n000"), Err(Failure::Unreachable));
        assert_eq!(sent(""), Err(Failure::Unreachable));
        assert_eq!(signed_in("{\"error\":\"Wrong name or password\"}\n401"), Err(Failure::SignIn("Wrong name or password".into())));
        assert_eq!(sent("{\"error\":\"The file changed on the way\"}\n400"), Err(Failure::Said("The file changed on the way".into())));
        assert_eq!(sent("<html>bad gateway</html>\n502"), Err(Failure::Said("The server answered 502".into())));
        assert_eq!(taken("\n204"), Ok(()));
    }

    #[test]
    fn a_pairs_jpeg_stays_behind_its_raw() {
        let paths: Vec<PathBuf> =
            ["/a/DSCF1.RAF", "/a/DSCF1.JPG", "/a/DSCF2.JPG", "/b/DSCF1.JPG", "/a/dscf3.raf", "/a/DSCF3.HEIF"].map(PathBuf::from).to_vec();
        assert_eq!(originals(&paths), vec![0, 2, 3, 4]);
    }
}
