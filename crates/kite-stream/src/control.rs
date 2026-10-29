//! GameStream/Sunshine control. Every connection here is an authenticated SSH channel.
//! Pairing follows the official Moonlight protocol; only Sunshine's modern SHA-256 generation is supported.

use crate::{StreamConfig, StreamError};
use base64::Engine;
use kite_ssh::Session;
use openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::Rsa,
    sign::{Signer, Verifier},
    ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion},
    symm::{Cipher, Crypter, Mode},
    x509::{X509, X509NameBuilder},
};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const LIMIT: u64 = 512 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

pub struct Identity {
    certificate: X509,
    key: PKey<Private>,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    certificate: String,
    private_key: String,
}

impl Identity {
    pub fn generate() -> Result<Self, StreamError> {
        let key = PKey::from_rsa(Rsa::generate(2048)?)?;
        let mut name = X509NameBuilder::new()?;
        name.append_entry_by_text("CN", "Kite Client")?;
        let name = name.build();
        let mut certificate = X509::builder()?;
        certificate.set_version(2)?;
        let serial = BigNum::from_slice(&random::<16>()?)?.to_asn1_integer()?;
        certificate.set_serial_number(&serial)?;
        certificate.set_subject_name(&name)?;
        certificate.set_issuer_name(&name)?;
        certificate.set_pubkey(&key)?;
        let before = Asn1Time::days_from_now(0)?;
        let after = Asn1Time::days_from_now(3650)?;
        certificate.set_not_before(&before)?;
        certificate.set_not_after(&after)?;
        certificate.sign(&key, MessageDigest::sha256())?;
        Ok(Self {
            certificate: certificate.build(),
            key,
        })
    }

    pub fn from_secret(bytes: &[u8]) -> Result<Self, StreamError> {
        let value: StoredIdentity = serde_json::from_slice(bytes)
            .map_err(|_| protocol("invalid saved streaming identity"))?;
        let certificate = X509::from_pem(value.certificate.as_bytes())?;
        let key = PKey::private_key_from_pem(value.private_key.as_bytes())?;
        if !certificate.public_key()?.public_eq(&key) {
            return Err(protocol("streaming identity key mismatch"));
        }
        Ok(Self { certificate, key })
    }

    pub fn to_secret(&self) -> Result<Vec<u8>, StreamError> {
        let value = StoredIdentity {
            certificate: String::from_utf8(self.certificate.to_pem()?)
                .map_err(|_| protocol("invalid certificate encoding"))?,
            private_key: String::from_utf8(self.key.private_key_to_pem_pkcs8()?)
                .map_err(|_| protocol("invalid key encoding"))?,
        };
        serde_json::to_vec(&value).map_err(|_| protocol("identity serialization failed"))
    }
}

#[derive(Clone)]
pub struct Client {
    ssh: Arc<Session>,
    host: String,
    port: u16,
    device_id: String,
    identity: Arc<Identity>,
    peer: Option<X509>,
    pairing_name: String,
}

#[derive(Clone, Debug)]
pub struct ServerInfo {
    pub name: String,
    pub app_version: String,
    pub gfe_version: String,
    pub codec_modes: i32,
    pub current_game: u32,
    pub paired: bool,
}

pub struct Launch {
    pub address: String,
    pub app_version: String,
    pub gfe_version: String,
    pub codec_modes: i32,
    pub rtsp_url: String,
    pub key: [u8; 16],
    pub iv: [u8; 16],
}

trait IoStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> IoStream for T {}

impl Client {
    pub fn pairing_name(&self) -> &str {
        &self.pairing_name
    }
    pub fn new(
        ssh: Arc<Session>,
        host: String,
        port: u16,
        device_id: String,
        identity: Arc<Identity>,
        peer_pem: Option<&str>,
    ) -> Result<Self, StreamError> {
        if !(6..=65514).contains(&port)
            || host.contains(['\r', '\n'])
            || device_id.len() != 16
            || !device_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(protocol("invalid Sunshine endpoint"));
        }
        let peer = peer_pem
            .map(|pem| X509::from_pem(pem.as_bytes()))
            .transpose()?;
        Ok(Self {
            ssh,
            host,
            port,
            device_id,
            identity,
            peer,
            pairing_name: format!("Kite-{}", new_device_id()?),
        })
    }

    pub fn certificate_pem(&self) -> Result<String, StreamError> {
        String::from_utf8(
            self.peer
                .as_ref()
                .ok_or(StreamError::PairingRejected)?
                .to_pem()?,
        )
        .map_err(|_| protocol("invalid server certificate encoding"))
    }

    async fn request(
        &self,
        tls: bool,
        port: u16,
        method: &str,
        path: &str,
        body: Option<(&str, &str)>,
        deadline: Duration,
    ) -> Result<String, StreamError> {
        tokio::time::timeout(deadline, async {
            let channel = self.ssh.open_tunnel(&self.host, port).await?;
            let mut stream: Box<dyn IoStream> = if tls {
                let mut builder = SslConnector::builder(SslMethod::tls_client())?;
                builder.set_min_proto_version(Some(SslVersion::TLS1_2))?;
                builder.set_certificate(&self.identity.certificate)?; builder.set_private_key(&self.identity.key)?;
                if port == self.port - 5 && let Some(peer) = &self.peer {
                    let expected = peer.to_der()?;
                    // Hostnames are not embedded in GameStream self-signed certificates.
                    // Require the exact certificate acquired through strict SSH and pairing.
                    builder.set_verify_callback(SslVerifyMode::PEER, move |_, ctx| {
                        ctx.error_depth() == 0 && ctx.current_cert().and_then(|c| c.to_der().ok()).is_some_and(|der| der == expected)
                    });
                } else {
                    // Bootstrap and the Web manager remain inside the verified SSH tunnel.
                    builder.set_verify(SslVerifyMode::NONE);
                }
                let mut config = builder.build().configure()?; config.set_verify_hostname(false);
                let ssl = config.into_ssl(&self.host)?;
                let mut tls = tokio_openssl::SslStream::new(ssl, channel)?;
                Pin::new(&mut tls).connect().await.map_err(|_| protocol("Sunshine TLS handshake or certificate verification failed"))?;
                Box::new(tls)
            } else { Box::new(channel) };
            let body_text = body.map(|(_, b)| b).unwrap_or("");
            let extra = body.map(|(headers, _)| headers).unwrap_or("");
            let request = format!("{method} {path} HTTP/1.0\r\nHost: {}:{port}\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body_text}", self.host, body_text.len());
            stream.write_all(request.as_bytes()).await?; stream.flush().await?;
            let mut bytes = Vec::new();
            stream.take(LIMIT + 1).read_to_end(&mut bytes).await?;
            if bytes.len() as u64 > LIMIT { return Err(protocol("response too large")); }
            parse_http(&bytes)
        }).await.map_err(|_| StreamError::Timeout)?
    }

    async fn get(
        &self,
        tls: bool,
        path: &str,
        query: &str,
        deadline: Duration,
    ) -> Result<String, StreamError> {
        let uuid = hex::encode(random::<16>()?);
        let path = format!("/{path}?uniqueid={}&uuid={uuid}&{query}", self.device_id);
        let value = self
            .request(
                tls,
                if tls { self.port - 5 } else { self.port },
                "GET",
                &path,
                None,
                deadline,
            )
            .await?;
        checked_xml(&value)?;
        Ok(value)
    }

    pub async fn info(&self) -> Result<ServerInfo, StreamError> {
        let xml = self
            .get(self.peer.is_some(), "serverinfo", "", DEADLINE)
            .await?;
        Ok(ServerInfo {
            name: field(&xml, "hostname").unwrap_or_default(),
            app_version: field(&xml, "appversion")?,
            gfe_version: field(&xml, "GfeVersion").unwrap_or_default(),
            codec_modes: field(&xml, "ServerCodecModeSupport")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            current_game: field(&xml, "currentgame")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            paired: field(&xml, "PairStatus").ok().as_deref() == Some("1"),
        })
    }

    pub async fn desktop_size(&self) -> Result<crate::display::DesktopSize, StreamError> {
        crate::display::captured_size(&self.ssh, self.port).await
    }

    pub async fn unpair(&self) -> Result<(), StreamError> {
        self.get(false, "unpair", "", DEADLINE).await?;
        Ok(())
    }

    pub async fn approve_pin_from_saved_credentials(&self, pin: &str) -> Result<(), StreamError> {
        #[derive(Deserialize)]
        struct Credentials {
            username: String,
            password: String,
        }
        let command = "cat \"$HOME/.config/sunshine/admin-credentials.json\"";
        let output = self.ssh.exec(command, &[]).await?;
        if output.exit_status != Some(0) || output.stdout.len() > 8192 {
            return Err(protocol("saved management credentials unavailable"));
        }
        let credentials: Credentials = serde_json::from_slice(&output.stdout)
            .map_err(|_| protocol("saved management credentials unavailable"))?;
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", credentials.username, credentials.password));
        let headers = format!("Authorization: Basic {token}\r\nContent-Type: application/json\r\n");
        let pending = self
            .request(
                true,
                self.port + 1,
                "GET",
                "/api/pin",
                Some((&headers, "")),
                DEADLINE,
            )
            .await?;
        let pending: serde_json::Value = serde_json::from_str(&pending)
            .map_err(|_| protocol("invalid pending pairings response"))?;
        let candidates = pending["pairings"]
            .as_array()
            .ok_or_else(|| protocol("missing pending pairings"))?
            .iter()
            .filter(|p| p["name"].as_str() == Some(&self.pairing_name))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(protocol("pairing request is not ready or is ambiguous"));
        }
        let id = candidates[0]["id"]
            .as_str()
            .ok_or_else(|| protocol("missing pairing ID"))?;
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(protocol("invalid pairing ID"));
        }
        let body = serde_json::json!({"pairing_id":id,"pin":pin,"name":"Kite"}).to_string();
        let reply = self
            .request(
                true,
                self.port + 1,
                "POST",
                "/api/pin",
                Some((&headers, &body)),
                DEADLINE,
            )
            .await?;
        let value: serde_json::Value =
            serde_json::from_str(&reply).map_err(|_| protocol("invalid management response"))?;
        if value["status"].as_bool() != Some(true) {
            return Err(StreamError::PairingRejected);
        }
        Ok(())
    }

    pub async fn pair(mut self, pin: &str) -> Result<Self, StreamError> {
        if pin.len() != 4 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return Err(protocol("invalid pairing PIN"));
        }
        let info = self.info().await?;
        if info
            .app_version
            .split('.')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            < 7
        {
            return Err(protocol("unsupported GameStream generation"));
        }
        let salt = random::<16>()?;
        let mut salted = salt.to_vec();
        salted.extend_from_slice(pin.as_bytes());
        let hash = openssl::sha::sha256(&salted);
        let key = &hash[..16];
        let query = format!(
            "devicename={}&updateState=1&phrase=getservercert&salt={}&clientcert={}",
            self.pairing_name,
            hex::encode(salt),
            hex::encode(self.identity.certificate.to_pem()?)
        );
        let cert_response = self
            .get(false, "pair", &query, Duration::from_secs(120))
            .await?;
        paired(&cert_response)?;
        let cert_bytes = decode_hex(&field(&cert_response, "plaincert")?)?;
        self.peer = Some(X509::from_pem(&cert_bytes)?);
        let challenge = random::<16>()?;
        let response = self
            .get(
                false,
                "pair",
                &format!(
                    "devicename=Kite&updateState=1&clientchallenge={}",
                    hex::encode(aes(&challenge, key, Mode::Encrypt)?)
                ),
                DEADLINE,
            )
            .await?;
        paired(&response)?;
        let decrypted = aes(
            &decode_hex(&field(&response, "challengeresponse")?)?,
            key,
            Mode::Decrypt,
        )?;
        if decrypted.len() != 48 {
            return Err(StreamError::PairingRejected);
        }
        let client_secret = random::<16>()?;
        let mut secret_hash = decrypted[32..48].to_vec();
        secret_hash.extend_from_slice(self.identity.certificate.signature().as_slice());
        secret_hash.extend_from_slice(&client_secret);
        let response = self
            .get(
                false,
                "pair",
                &format!(
                    "devicename=Kite&updateState=1&serverchallengeresp={}",
                    hex::encode(aes(
                        &openssl::sha::sha256(&secret_hash),
                        key,
                        Mode::Encrypt
                    )?)
                ),
                DEADLINE,
            )
            .await?;
        paired(&response)?;
        let signed_secret = decode_hex(&field(&response, "pairingsecret")?)?;
        if signed_secret.len() <= 16 {
            return Err(StreamError::PairingRejected);
        }
        let peer = self.peer.as_ref().ok_or(StreamError::PairingRejected)?;
        let peer_key = peer.public_key()?;
        let mut verify = Verifier::new(MessageDigest::sha256(), &peer_key)?;
        verify.update(&signed_secret[..16])?;
        if !verify.verify(&signed_secret[16..])? {
            return Err(StreamError::PairingRejected);
        }
        let mut expected = challenge.to_vec();
        expected.extend_from_slice(peer.signature().as_slice());
        expected.extend_from_slice(&signed_secret[..16]);
        if !openssl::memcmp::eq(&openssl::sha::sha256(&expected), &decrypted[..32]) {
            return Err(StreamError::PairingRejected);
        }
        let mut signer = Signer::new(MessageDigest::sha256(), &self.identity.key)?;
        signer.update(&client_secret)?;
        let mut client_pairing_secret = client_secret.to_vec();
        client_pairing_secret.extend_from_slice(&signer.sign_to_vec()?);
        let result = self
            .get(
                false,
                "pair",
                &format!(
                    "devicename=Kite&updateState=1&clientpairingsecret={}",
                    hex::encode(client_pairing_secret)
                ),
                DEADLINE,
            )
            .await?;
        paired(&result)?;
        let result = self
            .get(
                true,
                "pair",
                "devicename=Kite&updateState=1&phrase=pairchallenge",
                DEADLINE,
            )
            .await?;
        paired(&result)?;
        Ok(self)
    }

    pub async fn launch_desktop(&self, config: StreamConfig) -> Result<Launch, StreamError> {
        config.validate()?;
        let info = self.info().await?;
        if !info.paired {
            return Err(StreamError::PairingRejected);
        }
        let list = self.get(true, "applist", "", DEADLINE).await?;
        let doc = checked_xml(&list)?;
        let desktop = doc
            .descendants()
            .filter(|n| n.has_tag_name("App"))
            .find_map(|n| {
                let name = n.children().find(|c| c.has_tag_name("AppTitle"))?.text()?;
                if !name.eq_ignore_ascii_case("Desktop") {
                    return None;
                }
                n.children()
                    .find(|c| c.has_tag_name("ID"))?
                    .text()?
                    .parse::<u32>()
                    .ok()
            })
            .ok_or(StreamError::DesktopMissing)?;
        if info.current_game != 0 && info.current_game != desktop {
            return Err(protocol("a different application is running on the host"));
        }
        let address = private_address(&self.host, self.port).await?;
        let key = random::<16>()?;
        let id = u32::from_be_bytes(random::<4>()?);
        let mut iv = [0; 16];
        iv[..4].copy_from_slice(&id.to_be_bytes());
        let query = format!(
            "appid={desktop}&mode={}x{}x{}&additionalStates=1&sops=0&rikey={}&rikeyid={id}&localAudioPlayMode=0&surroundAudioInfo=196610&remoteControllersBitmap=0&gcmap=0&gcpersist=0{}",
            config.width,
            config.height,
            config.fps,
            hex::encode(key),
            crate::native::launch_query()
        );
        let verb = if info.current_game == desktop {
            "resume"
        } else {
            "launch"
        };
        let reply = self.get(true, verb, &query, DEADLINE).await?;
        let started = field(
            &reply,
            if verb == "launch" {
                "gamesession"
            } else {
                "resume"
            },
        )?;
        if started == "0" {
            return Err(protocol("desktop launch rejected"));
        }
        let rtsp_url = field(&reply, "sessionUrl0")?;
        Ok(Launch {
            address,
            app_version: info.app_version,
            gfe_version: info.gfe_version,
            codec_modes: info.codec_modes,
            rtsp_url,
            key,
            iv,
        })
    }
}

pub fn new_device_id() -> Result<String, StreamError> {
    Ok(hex::encode(random::<8>()?))
}
pub fn new_pin() -> Result<String, StreamError> {
    Ok(format!("{:04}", u32::from_be_bytes(random::<4>()?) % 10000))
}
fn random<const N: usize>() -> Result<[u8; N], StreamError> {
    let mut bytes = [0; N];
    openssl::rand::rand_bytes(&mut bytes)?;
    Ok(bytes)
}
fn protocol(details: impl Into<String>) -> StreamError {
    StreamError::Protocol(details.into())
}
fn decode_hex(value: &str) -> Result<Vec<u8>, StreamError> {
    hex::decode(value).map_err(|_| protocol("invalid hexadecimal response"))
}
fn paired(xml: &str) -> Result<(), StreamError> {
    if field(xml, "paired")? == "1" {
        Ok(())
    } else {
        Err(StreamError::PairingRejected)
    }
}

fn checked_xml(xml: &str) -> Result<roxmltree::Document<'_>, StreamError> {
    let doc = roxmltree::Document::parse(xml).map_err(|_| protocol("malformed XML"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "root" || root.attribute("status_code") != Some("200") {
        return Err(protocol("GameStream request rejected"));
    }
    Ok(doc)
}
fn field(xml: &str, name: &str) -> Result<String, StreamError> {
    let doc = checked_xml(xml)?;
    doc.descendants()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .map(str::to_owned)
        .ok_or_else(|| protocol(format!("missing {name}")))
}
fn aes(input: &[u8], key: &[u8], mode: Mode) -> Result<Vec<u8>, StreamError> {
    if !input.len().is_multiple_of(16) {
        return Err(StreamError::PairingRejected);
    }
    let mut c = Crypter::new(Cipher::aes_128_ecb(), mode, key, None)?;
    c.pad(false);
    let mut output = vec![0; input.len() + 16];
    let n = c.update(input, &mut output)?;
    let m = c.finalize(&mut output[n..])?;
    output.truncate(n + m);
    Ok(output)
}

pub async fn private_address(host: &str, port: u16) -> Result<String, StreamError> {
    let addresses = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| StreamError::Timeout)??;
    for a in addresses {
        let ip = a.ip();
        let private = match ip {
            std::net::IpAddr::V4(v) => {
                v.is_private()
                    || v.is_loopback()
                    || (v.octets()[0] == 100 && (64..=127).contains(&v.octets()[1]))
            }
            std::net::IpAddr::V6(v) => {
                v.is_unique_local() || v.is_loopback() || v.is_unicast_link_local()
            }
        };
        if private {
            return Ok(ip.to_string());
        }
    }
    Err(StreamError::PublicAddress)
}

fn parse_http(bytes: &[u8]) -> Result<String, StreamError> {
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| protocol("incomplete HTTP headers"))?;
    let headers =
        std::str::from_utf8(&bytes[..split]).map_err(|_| protocol("invalid HTTP headers"))?;
    let code = headers
        .lines()
        .next()
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| protocol("invalid HTTP status"))?;
    if !(200..300).contains(&code) {
        return Err(StreamError::Http(code));
    }
    if headers
        .lines()
        .any(|s| s.to_ascii_lowercase().starts_with("transfer-encoding:"))
    {
        return Err(protocol("unexpected chunked HTTP/1.0 response"));
    }
    let body = &bytes[split + 4..];
    if let Some(length) = headers.lines().find_map(|s| {
        let (k, v) = s.split_once(':')?;
        if k.eq_ignore_ascii_case("content-length") {
            v.trim().parse::<usize>().ok()
        } else {
            None
        }
    }) && body.len() != length
    {
        return Err(protocol("incomplete HTTP body"));
    }
    String::from_utf8(body.to_vec()).map_err(|_| protocol("invalid HTTP body encoding"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_round_trip_and_sign_challenges() {
        let identity = Identity::generate().unwrap();
        let restored = Identity::from_secret(&identity.to_secret().unwrap()).unwrap();
        assert_eq!(
            identity.certificate.to_der().unwrap(),
            restored.certificate.to_der().unwrap()
        );
        let bytes = [42; 16];
        let mut s = Signer::new(MessageDigest::sha256(), &identity.key).unwrap();
        s.update(&bytes).unwrap();
        let signature = s.sign_to_vec().unwrap();
        let key = restored.certificate.public_key().unwrap();
        let mut v = Verifier::new(MessageDigest::sha256(), &key).unwrap();
        v.update(&bytes).unwrap();
        assert!(v.verify(&signature).unwrap());
    }
    #[test]
    fn pairing_aes_uses_ecb_without_padding() {
        let data = [7; 48];
        let key = [3; 16];
        let enc = aes(&data, &key, Mode::Encrypt).unwrap();
        assert_eq!(enc.len(), 48);
        assert_eq!(aes(&enc, &key, Mode::Decrypt).unwrap(), data);
    }
    #[test]
    fn rejects_truncated_http_and_xml_errors() {
        assert!(parse_http(b"HTTP/1.0 200 OK\r\nContent-Length: 9\r\n\r\nshort").is_err());
        assert!(checked_xml("<root status_code=\"401\"/>").is_err());
        assert_eq!(
            field(
                "<root status_code=\"200\"><name>A &amp; B</name></root>",
                "name"
            )
            .unwrap(),
            "A & B"
        );
    }
}
