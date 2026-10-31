use super::{Error, NodeRequest, Result, wire};
use std::{process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    process::Command,
    sync::Mutex,
};

pub async fn server<R, W>(mut input: R, output: W, mut command: Command) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or(Error::Protocol("missing stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(Error::Protocol("missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(Error::Protocol("missing stderr"))?;
    let writer = Arc::new(Mutex::new(output));
    let output_task = tokio::spawn(copy_output(stdout, writer.clone(), wire::OUTPUT));
    let error_task = tokio::spawn(copy_output(stderr, writer.clone(), wire::STDERR));
    let mut input_task = tokio::spawn(async move {
        while let Some((kind, data)) = wire::read(&mut input).await? {
            match kind {
                wire::INPUT => stdin.write_all(&data).await?,
                wire::EOF if data.is_empty() => {
                    stdin.shutdown().await?;
                    return Ok::<bool, Error>(true);
                }
                _ => return Err(Error::Protocol("invalid client frame")),
            }
        }
        Ok(false)
    });
    let status = tokio::select! {
        status = child.wait() => status?,
        closed = &mut input_task => {
            if !matches!(closed, Ok(Ok(true))) { let _ = child.kill().await; }
            child.wait().await?
        }
    };
    input_task.abort();
    output_task
        .await
        .map_err(|_| Error::Protocol("stdout task failed"))??;
    error_task
        .await
        .map_err(|_| Error::Protocol("stderr task failed"))??;
    wire::write(
        &mut *writer.lock().await,
        wire::EXIT,
        &status.code().unwrap_or(1).to_be_bytes(),
    )
    .await
}
async fn copy_output<R, W>(mut source: R, output: Arc<Mutex<W>>, kind: u8) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut chunk = vec![0; wire::FRAME];
    loop {
        let n = source.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        wire::write(&mut *output.lock().await, kind, &chunk[..n]).await?;
    }
}

pub async fn client(socket: &std::path::Path, request: NodeRequest) -> Result<i32> {
    let stream = UnixStream::connect(socket)
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?;
    let (read, mut write) = stream.into_split();
    super::json_line(&mut write, &request).await?;
    let mut read = tokio::io::BufReader::new(read);
    let ack = super::read_json(&mut read).await?;
    if ack["ok"] != true {
        return Err(Error::Remote(
            ack["error"].as_str().unwrap_or("bridge rejected").into(),
        ));
    }
    let sender = tokio::spawn(async move {
        let mut input = tokio::io::stdin();
        let mut bytes = vec![0; wire::FRAME];
        loop {
            let n = input.read(&mut bytes).await?;
            if n == 0 {
                wire::write(&mut write, wire::EOF, &[]).await?;
                break;
            }
            wire::write(&mut write, wire::INPUT, &bytes[..n]).await?;
        }
        Ok::<(), Error>(())
    });
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    let result = loop {
        let Some((kind, bytes)) = wire::read(&mut read).await? else {
            break Err(Error::Protocol("bridge closed without exit status"));
        };
        match kind {
            wire::OUTPUT => {
                stdout.write_all(&bytes).await?;
                stdout.flush().await?;
            }
            wire::STDERR => {
                stderr.write_all(&bytes).await?;
                stderr.flush().await?;
            }
            wire::EXIT if bytes.len() == 4 => {
                break Ok(i32::from_be_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| Error::Protocol("invalid exit status"))?,
                ));
            }
            _ => break Err(Error::Protocol("invalid server frame")),
        }
    };
    sender.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn child_input_output_error_and_exit_are_preserved() {
        let (mut client, server_side) = tokio::io::duplex(4096);
        let (input, output) = tokio::io::split(server_side);
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "cat; printf diagnostic >&2; exit 17"]);
        let task = tokio::spawn(server(input, output, cmd));
        wire::write(&mut client, wire::INPUT, b"binary\0input")
            .await
            .unwrap();
        wire::write(&mut client, wire::EOF, &[]).await.unwrap();
        let mut out = vec![];
        let mut err = vec![];
        loop {
            let (kind, bytes) = wire::read(&mut client).await.unwrap().unwrap();
            match kind {
                wire::OUTPUT => out.extend(bytes),
                wire::STDERR => err.extend(bytes),
                wire::EXIT => {
                    assert_eq!(bytes, 17i32.to_be_bytes());
                    break;
                }
                _ => panic!("unexpected frame"),
            }
        }
        task.await.unwrap().unwrap();
        assert_eq!(out, b"binary\0input");
        assert_eq!(err, b"diagnostic");
    }
}
