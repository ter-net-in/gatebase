use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::{timeout, Duration};

const SSL_REQUEST: i32 = 80877103;
const CANCEL_REQUEST: i32 = 80877102;
const PROTOCOL_VERSION_3: i32 = 196608;
const IO_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct FrontendMessage {
    pub(crate) tag: u8,
    pub(crate) body: Vec<u8>,
}

pub(crate) async fn read_startup(
    reader: &mut OwnedReadHalf,
    writer: &mut OwnedWriteHalf,
) -> Result<Vec<u8>> {
    loop {
        let mut len = [0_u8; 4];
        timeout(IO_TIMEOUT, reader.read_exact(&mut len)).await??;
        let len = i32::from_be_bytes(len);
        anyhow::ensure!(
            (8..=100_000).contains(&len),
            "invalid startup packet length"
        );
        let mut body = vec![0_u8; (len - 4) as usize];
        timeout(IO_TIMEOUT, reader.read_exact(&mut body)).await??;
        let code = i32::from_be_bytes(body[0..4].try_into()?);
        match code {
            SSL_REQUEST => writer.write_all(b"N").await?,
            CANCEL_REQUEST => return Err(anyhow!("CancelRequest is not supported yet")),
            PROTOCOL_VERSION_3 => return Ok(body[4..].to_vec()),
            _ => return Err(anyhow!("unsupported Postgres protocol version {code}")),
        }
    }
}

pub(crate) fn parse_startup(body: &[u8]) -> Result<std::collections::HashMap<String, String>> {
    let mut params = std::collections::HashMap::new();
    let mut parts = body
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty());
    while let Some(key) = parts.next() {
        let Some(value) = parts.next() else {
            return Err(anyhow!("startup parameter without value"));
        };
        params.insert(
            String::from_utf8(key.to_vec())?,
            String::from_utf8(value.to_vec())?,
        );
    }
    Ok(params)
}

pub(crate) async fn request_password(
    reader: &mut OwnedReadHalf,
    writer: &mut OwnedWriteHalf,
) -> Result<String> {
    write_message(writer, b'R', &3_i32.to_be_bytes()).await?;
    let message = read_message(reader).await?;
    anyhow::ensure!(message.tag == b'p', "expected PasswordMessage");
    Ok(cstring(&message.body)?.to_owned())
}

pub(crate) async fn read_message(reader: &mut OwnedReadHalf) -> Result<FrontendMessage> {
    let mut tag = [0_u8; 1];
    timeout(IO_TIMEOUT, reader.read_exact(&mut tag)).await??;
    let mut len = [0_u8; 4];
    timeout(IO_TIMEOUT, reader.read_exact(&mut len)).await??;
    let len = i32::from_be_bytes(len);
    anyhow::ensure!(
        (4..=10_000_000).contains(&len),
        "invalid frontend message length"
    );
    let mut body = vec![0_u8; (len - 4) as usize];
    timeout(IO_TIMEOUT, reader.read_exact(&mut body)).await??;
    Ok(FrontendMessage { tag: tag[0], body })
}

pub(crate) fn is_clean_disconnect(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::UnexpectedEof)
}

pub(crate) fn cstring(body: &[u8]) -> Result<&str> {
    let end = body
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(body.len());
    Ok(std::str::from_utf8(&body[..end])?)
}

pub(crate) fn cstring_with_remainder(body: &[u8]) -> Result<(&str, &[u8])> {
    let end = body
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| anyhow!("missing null terminator"))?;
    Ok((std::str::from_utf8(&body[..end])?, &body[end + 1..]))
}

pub(crate) fn parse_statement_message(body: &[u8]) -> Result<(&str, &str)> {
    let (name, remainder) = cstring_with_remainder(body)?;
    let (statement, _) = cstring_with_remainder(remainder)?;
    Ok((name, statement))
}

pub(crate) fn parse_bind_message(body: &[u8]) -> Result<(&str, &str, i16)> {
    let (portal, remainder) = cstring_with_remainder(body)?;
    let (statement, remainder) = cstring_with_remainder(remainder)?;
    anyhow::ensure!(remainder.len() >= 2, "invalid Bind message");
    let format_count = i16::from_be_bytes(remainder[0..2].try_into()?);
    anyhow::ensure!(format_count >= 0, "invalid Bind message");
    let mut offset = 2 + (format_count as usize * 2);
    anyhow::ensure!(remainder.len() >= offset + 2, "invalid Bind message");
    let parameter_count = i16::from_be_bytes(remainder[offset..offset + 2].try_into()?);
    anyhow::ensure!(parameter_count >= 0, "invalid Bind message");
    offset += 2;
    for _ in 0..parameter_count {
        anyhow::ensure!(remainder.len() >= offset + 4, "invalid Bind message");
        let len = i32::from_be_bytes(remainder[offset..offset + 4].try_into()?);
        offset += 4;
        if len >= 0 {
            offset += len as usize;
            anyhow::ensure!(remainder.len() >= offset, "invalid Bind message");
        }
    }
    Ok((portal, statement, parameter_count))
}

pub(crate) fn parse_execute_message(body: &[u8]) -> Result<&str> {
    let (portal, remainder) = cstring_with_remainder(body)?;
    anyhow::ensure!(remainder.len() >= 4, "invalid Execute message");
    Ok(portal)
}

pub(crate) fn parse_describe_message(body: &[u8]) -> Result<(u8, &str)> {
    anyhow::ensure!(body.len() >= 2, "invalid Describe message");
    let describe_type = body[0];
    let (name, _) = cstring_with_remainder(&body[1..])?;
    Ok((describe_type, name))
}

pub(crate) fn parse_close_message(body: &[u8]) -> Result<(u8, &str)> {
    anyhow::ensure!(body.len() >= 2, "invalid Close message");
    let close_type = body[0];
    let (name, _) = cstring_with_remainder(&body[1..])?;
    Ok((close_type, name))
}

pub(crate) async fn write_auth_ok(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'R', &0_i32.to_be_bytes()).await
}

pub(crate) async fn write_parameter_status(
    writer: &mut OwnedWriteHalf,
    key: &str,
    value: &str,
) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(key.as_bytes());
    body.push(0);
    body.extend_from_slice(value.as_bytes());
    body.push(0);
    write_message(writer, b'S', &body).await
}

pub(crate) async fn write_backend_key_data(writer: &mut OwnedWriteHalf) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&0_i32.to_be_bytes());
    body.extend_from_slice(&0_i32.to_be_bytes());
    write_message(writer, b'K', &body).await
}

pub(crate) async fn write_ready(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'Z', b"I").await
}

pub(crate) async fn write_parse_complete(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'1', &[]).await
}

pub(crate) async fn write_bind_complete(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'2', &[]).await
}

pub(crate) async fn write_close_complete(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'3', &[]).await
}

pub(crate) async fn write_no_data(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'n', &[]).await
}

pub(crate) async fn write_empty_parameter_description(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b't', &0_i16.to_be_bytes()).await
}

pub(crate) async fn write_empty_query(writer: &mut OwnedWriteHalf) -> Result<()> {
    write_message(writer, b'I', &[]).await
}

pub(crate) async fn write_row_description(
    writer: &mut OwnedWriteHalf,
    columns: &[tokio_postgres::SimpleColumn],
) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&(columns.len() as i16).to_be_bytes());
    for column in columns {
        body.extend_from_slice(column.name().as_bytes());
        body.push(0);
        body.extend_from_slice(&0_i32.to_be_bytes());
        body.extend_from_slice(&0_i16.to_be_bytes());
        body.extend_from_slice(&25_i32.to_be_bytes());
        body.extend_from_slice(&(-1_i16).to_be_bytes());
        body.extend_from_slice(&(-1_i32).to_be_bytes());
        body.extend_from_slice(&0_i16.to_be_bytes());
    }
    write_message(writer, b'T', &body).await
}

pub(crate) async fn write_data_row(
    writer: &mut OwnedWriteHalf,
    row: &tokio_postgres::SimpleQueryRow,
) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&(row.len() as i16).to_be_bytes());
    for index in 0..row.len() {
        match row.get(index) {
            Some(value) => {
                body.extend_from_slice(&(value.len() as i32).to_be_bytes());
                body.extend_from_slice(value.as_bytes());
            }
            None => body.extend_from_slice(&(-1_i32).to_be_bytes()),
        }
    }
    write_message(writer, b'D', &body).await
}

pub(crate) async fn write_command_complete(
    writer: &mut OwnedWriteHalf,
    statement: &str,
    count: u64,
) -> Result<()> {
    let command = statement
        .split_whitespace()
        .next()
        .unwrap_or("OK")
        .to_ascii_uppercase();
    let tag = match command.as_str() {
        "SELECT" => format!("SELECT {count}"),
        "INSERT" => format!("INSERT 0 {count}"),
        "UPDATE" | "DELETE" | "MOVE" | "FETCH" | "COPY" => format!("{command} {count}"),
        _ => command,
    };
    let mut body = tag.into_bytes();
    body.push(0);
    write_message(writer, b'C', &body).await
}

pub(crate) async fn write_error(
    writer: &mut OwnedWriteHalf,
    code: &str,
    message: &str,
) -> Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(b"SERROR\0");
    body.push(b'C');
    body.extend_from_slice(code.as_bytes());
    body.push(0);
    body.push(b'M');
    body.extend_from_slice(message.as_bytes());
    body.push(0);
    body.push(0);
    write_message(writer, b'E', &body).await
}

async fn write_message(writer: &mut OwnedWriteHalf, tag: u8, body: &[u8]) -> Result<()> {
    let len = (body.len() + 4) as i32;
    writer.write_all(&[tag]).await?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(body).await?;
    Ok(())
}
