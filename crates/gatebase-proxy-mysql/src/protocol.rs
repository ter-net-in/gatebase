use anyhow::{anyhow, Result};
use mysql_async::{Row, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const IO_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_CONNECT_WITH_DB: u32 = 0x0000_0008;
const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA: u32 = 0x0020_0000;
const SERVER_STATUS_AUTOCOMMIT: u16 = 0x0002;

pub(crate) const COM_QUIT: u8 = 0x01;
pub(crate) const COM_QUERY: u8 = 0x03;
pub(crate) const COM_PING: u8 = 0x0e;

pub(crate) struct ClientLogin {
    pub(crate) username: String,
    pub(crate) database: Option<String>,
    pub(crate) token: String,
    pub(crate) ok_sequence: u8,
}

struct LoginAttempt {
    username: String,
    database: Option<String>,
    auth_response: Vec<u8>,
    plugin: Option<String>,
}

pub(crate) struct ClientPacket {
    pub(crate) sequence: u8,
    pub(crate) payload: Vec<u8>,
}

pub(crate) async fn handshake(stream: &mut TcpStream) -> Result<ClientLogin> {
    let seed = b"gatebase_seed_for_auth";
    let capabilities = CLIENT_PROTOCOL_41
        | CLIENT_SECURE_CONNECTION
        | CLIENT_PLUGIN_AUTH
        | CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA
        | CLIENT_CONNECT_WITH_DB;
    let mut payload = Vec::new();
    payload.push(10);
    payload.extend_from_slice(b"8.0.0-gatebase\0");
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&seed[..8]);
    payload.push(0);
    payload.extend_from_slice(&(capabilities as u16).to_le_bytes());
    payload.push(45);
    payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&((capabilities >> 16) as u16).to_le_bytes());
    payload.push(21);
    payload.extend_from_slice(&[0_u8; 10]);
    payload.extend_from_slice(&seed[8..]);
    payload.push(0);
    payload.extend_from_slice(b"mysql_clear_password\0");
    write_packet(stream, 0, &payload).await?;

    let login_packet = read_packet(stream).await?;
    let login = parse_login(&login_packet.payload)?;
    if login.plugin.as_deref() == Some("mysql_clear_password") {
        return Ok(ClientLogin {
            username: login.username,
            database: login.database,
            token: clear_password_token(&login.auth_response)?,
            ok_sequence: login_packet.sequence.wrapping_add(1),
        });
    }

    write_auth_switch(stream, login_packet.sequence.wrapping_add(1), seed).await?;
    let token_packet = read_packet(stream).await?;
    Ok(ClientLogin {
        username: login.username,
        database: login.database,
        token: clear_password_token(&token_packet.payload)?,
        ok_sequence: token_packet.sequence.wrapping_add(1),
    })
}

pub(crate) async fn read_packet(stream: &mut TcpStream) -> Result<ClientPacket> {
    let mut header = [0_u8; 4];
    timeout(IO_TIMEOUT, stream.read_exact(&mut header)).await??;
    let len =
        usize::from(header[0]) | (usize::from(header[1]) << 8) | (usize::from(header[2]) << 16);
    anyhow::ensure!(len <= 16 * 1024 * 1024, "invalid MySQL packet length");
    let mut payload = vec![0_u8; len];
    timeout(IO_TIMEOUT, stream.read_exact(&mut payload)).await??;
    Ok(ClientPacket {
        sequence: header[3],
        payload,
    })
}

pub(crate) async fn write_ok(
    stream: &mut TcpStream,
    sequence: u8,
    affected_rows: u64,
) -> Result<()> {
    let mut payload = vec![0x00];
    put_lenenc_int(&mut payload, affected_rows);
    put_lenenc_int(&mut payload, 0);
    payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    write_packet(stream, sequence, &payload).await
}

pub(crate) async fn write_err(
    stream: &mut TcpStream,
    sequence: u8,
    code: u16,
    message: &str,
) -> Result<()> {
    let mut payload = vec![0xff];
    payload.extend_from_slice(&code.to_le_bytes());
    payload.push(b'#');
    payload.extend_from_slice(b"HY000");
    payload.extend_from_slice(message.as_bytes());
    write_packet(stream, sequence, &payload).await
}

pub(crate) async fn write_result_set(
    stream: &mut TcpStream,
    columns: Arc<[mysql_async::Column]>,
    rows: Vec<Row>,
) -> Result<()> {
    let mut sequence = 1_u8;
    let mut count = Vec::new();
    put_lenenc_int(&mut count, columns.len() as u64);
    write_packet(stream, sequence, &count).await?;
    sequence = sequence.wrapping_add(1);

    for column in columns.iter() {
        write_packet(
            stream,
            sequence,
            &column_definition(column.name_str().as_bytes()),
        )
        .await?;
        sequence = sequence.wrapping_add(1);
    }
    write_eof(stream, sequence).await?;
    sequence = sequence.wrapping_add(1);

    for row in rows {
        write_packet(stream, sequence, &row_payload(&row)).await?;
        sequence = sequence.wrapping_add(1);
    }
    write_eof(stream, sequence).await
}

pub(crate) async fn write_packet(
    stream: &mut TcpStream,
    sequence: u8,
    payload: &[u8],
) -> Result<()> {
    anyhow::ensure!(payload.len() <= 0x00ff_ffff, "MySQL packet too large");
    let len = payload.len();
    let header = [
        (len & 0xff) as u8,
        ((len >> 8) & 0xff) as u8,
        ((len >> 16) & 0xff) as u8,
        sequence,
    ];
    stream.write_all(&header).await?;
    stream.write_all(payload).await?;
    Ok(())
}

fn parse_login(payload: &[u8]) -> Result<LoginAttempt> {
    anyhow::ensure!(payload.len() >= 36, "invalid MySQL login packet");
    let capabilities = u32::from_le_bytes(payload[0..4].try_into()?);
    let mut offset = 32;
    let (username, next) = read_null_string(payload, offset)?;
    offset = next;

    let (auth_response, next) = if capabilities & CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA != 0 {
        let (len, next) = read_lenenc_int(payload, offset)?;
        offset = next;
        let end = offset + len as usize;
        anyhow::ensure!(end <= payload.len(), "invalid MySQL auth response");
        (&payload[offset..end], end)
    } else if capabilities & CLIENT_SECURE_CONNECTION != 0 {
        let len = *payload
            .get(offset)
            .ok_or_else(|| anyhow!("missing auth response"))? as usize;
        offset += 1;
        let end = offset + len;
        anyhow::ensure!(end <= payload.len(), "invalid MySQL auth response");
        (&payload[offset..end], end)
    } else {
        let (value, next) = read_null_bytes(payload, offset)?;
        (value, next)
    };
    offset = next;

    let database = if capabilities & CLIENT_CONNECT_WITH_DB != 0 && offset < payload.len() {
        let (database, next) = read_null_string(payload, offset)?;
        offset = next;
        if database.is_empty() {
            None
        } else {
            Some(database)
        }
    } else {
        None
    };

    let plugin = if capabilities & CLIENT_PLUGIN_AUTH != 0 && offset < payload.len() {
        Some(read_null_string(payload, offset)?.0)
    } else {
        None
    };

    Ok(LoginAttempt {
        username,
        database,
        auth_response: auth_response.to_vec(),
        plugin,
    })
}

async fn write_auth_switch(stream: &mut TcpStream, sequence: u8, seed: &[u8]) -> Result<()> {
    let mut payload = vec![0xfe];
    payload.extend_from_slice(b"mysql_clear_password\0");
    payload.extend_from_slice(seed);
    payload.push(0);
    write_packet(stream, sequence, &payload).await
}

fn clear_password_token(payload: &[u8]) -> Result<String> {
    let token = payload.strip_suffix(&[0]).unwrap_or(payload);
    Ok(String::from_utf8(token.to_vec())?)
}

fn column_definition(name: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    put_lenenc_str(&mut payload, b"def");
    put_lenenc_str(&mut payload, b"");
    put_lenenc_str(&mut payload, b"");
    put_lenenc_str(&mut payload, b"");
    put_lenenc_str(&mut payload, name);
    put_lenenc_str(&mut payload, b"");
    payload.push(0x0c);
    payload.extend_from_slice(&33_u16.to_le_bytes());
    payload.extend_from_slice(&1024_u32.to_le_bytes());
    payload.push(0xfd);
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&[0_u8; 2]);
    payload
}

fn row_payload(row: &Row) -> Vec<u8> {
    let mut payload = Vec::new();
    for index in 0..row.len() {
        match row.as_ref(index) {
            Some(Value::NULL) | None => payload.push(0xfb),
            Some(value) => put_lenenc_str(&mut payload, &value_to_bytes(value)),
        }
    }
    payload
}

fn value_to_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::NULL => Vec::new(),
        Value::Bytes(value) => value.clone(),
        Value::Int(value) => value.to_string().into_bytes(),
        Value::UInt(value) => value.to_string().into_bytes(),
        Value::Float(value) => value.to_string().into_bytes(),
        Value::Double(value) => value.to_string().into_bytes(),
        Value::Date(year, month, day, hour, minute, second, micros) => {
            if *hour == 0 && *minute == 0 && *second == 0 && *micros == 0 {
                format!("{year:04}-{month:02}-{day:02}").into_bytes()
            } else if *micros == 0 {
                format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
                    .into_bytes()
            } else {
                format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{micros:06}"
                )
                .into_bytes()
            }
        }
        Value::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if *negative { "-" } else { "" };
            let hours = days * 24 + u32::from(*hours);
            if *micros == 0 {
                format!("{sign}{hours:03}:{minutes:02}:{seconds:02}").into_bytes()
            } else {
                format!("{sign}{hours:03}:{minutes:02}:{seconds:02}.{micros:06}").into_bytes()
            }
        }
    }
}

async fn write_eof(stream: &mut TcpStream, sequence: u8) -> Result<()> {
    let mut payload = vec![0xfe, 0, 0];
    payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    write_packet(stream, sequence, &payload).await
}

fn put_lenenc_str(payload: &mut Vec<u8>, value: &[u8]) {
    put_lenenc_int(payload, value.len() as u64);
    payload.extend_from_slice(value);
}

fn put_lenenc_int(payload: &mut Vec<u8>, value: u64) {
    match value {
        0..=250 => payload.push(value as u8),
        251..=0xffff => {
            payload.push(0xfc);
            payload.extend_from_slice(&(value as u16).to_le_bytes());
        }
        0x1_0000..=0xff_ffff => {
            payload.push(0xfd);
            payload.extend_from_slice(&[
                (value & 0xff) as u8,
                ((value >> 8) & 0xff) as u8,
                ((value >> 16) & 0xff) as u8,
            ]);
        }
        _ => {
            payload.push(0xfe);
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn read_lenenc_int(payload: &[u8], offset: usize) -> Result<(u64, usize)> {
    let first = *payload
        .get(offset)
        .ok_or_else(|| anyhow!("missing length-encoded integer"))?;
    match first {
        0xfc => {
            anyhow::ensure!(
                offset + 3 <= payload.len(),
                "invalid length-encoded integer"
            );
            Ok((
                u16::from_le_bytes(payload[offset + 1..offset + 3].try_into()?) as u64,
                offset + 3,
            ))
        }
        0xfd => {
            anyhow::ensure!(
                offset + 4 <= payload.len(),
                "invalid length-encoded integer"
            );
            Ok((
                u64::from(payload[offset + 1])
                    | (u64::from(payload[offset + 2]) << 8)
                    | (u64::from(payload[offset + 3]) << 16),
                offset + 4,
            ))
        }
        0xfe => {
            anyhow::ensure!(
                offset + 9 <= payload.len(),
                "invalid length-encoded integer"
            );
            Ok((
                u64::from_le_bytes(payload[offset + 1..offset + 9].try_into()?),
                offset + 9,
            ))
        }
        value => Ok((u64::from(value), offset + 1)),
    }
}

fn read_null_string(payload: &[u8], offset: usize) -> Result<(String, usize)> {
    let (bytes, next) = read_null_bytes(payload, offset)?;
    Ok((String::from_utf8(bytes.to_vec())?, next))
}

fn read_null_bytes(payload: &[u8], offset: usize) -> Result<(&[u8], usize)> {
    let rest = payload
        .get(offset..)
        .ok_or_else(|| anyhow!("invalid packet offset"))?;
    let end = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| anyhow!("missing null terminator"))?;
    Ok((&rest[..end], offset + end + 1))
}
