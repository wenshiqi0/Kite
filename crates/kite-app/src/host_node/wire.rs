use super::{Error, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const FRAME: usize = 64 * 1024;
pub const INPUT: u8 = 0;
pub const OUTPUT: u8 = 1;
pub const STDERR: u8 = 2;
pub const EXIT: u8 = 3;
pub const EOF: u8 = 4;

pub async fn write<W: AsyncWrite + Unpin>(out: &mut W, kind: u8, bytes: &[u8]) -> Result<()> {
    if bytes.len() > FRAME {
        return Err(Error::Protocol("frame too large"));
    }
    out.write_u8(kind).await?;
    out.write_u32(bytes.len() as u32).await?;
    out.write_all(bytes).await?;
    out.flush().await?;
    Ok(())
}
pub async fn read<R: AsyncRead + Unpin>(input: &mut R) -> Result<Option<(u8, Vec<u8>)>> {
    let kind = match input.read_u8().await {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let length = input.read_u32().await? as usize;
    if length > FRAME {
        return Err(Error::Protocol("frame too large"));
    }
    let mut data = vec![0; length];
    input.read_exact(&mut data).await?;
    Ok(Some((kind, data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn binary_frames_preserve_gzip_and_exit_payloads() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let bytes = vec![0, 255, 31, 139, 0, 10];
        write(&mut a, OUTPUT, &bytes).await.unwrap();
        write(&mut a, EXIT, &17i32.to_be_bytes()).await.unwrap();
        assert_eq!(read(&mut b).await.unwrap(), Some((OUTPUT, bytes)));
        assert_eq!(
            read(&mut b).await.unwrap(),
            Some((EXIT, 17i32.to_be_bytes().to_vec()))
        );
    }
    #[tokio::test]
    async fn oversized_input_is_rejected_before_allocation() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_u8(INPUT).await.unwrap();
        a.write_u32((FRAME + 1) as u32).await.unwrap();
        assert!(matches!(read(&mut b).await, Err(Error::Protocol(_))));
    }
}
