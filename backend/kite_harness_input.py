"""Convert uploaded local images to native vision blocks, within owned storage."""
import base64
import os
from pathlib import Path
import re
import stat


def image(path):
    path = Path(path)
    root = Path.home() / ".local/share/kite/uploads"
    if path.parent.parent != root or not re.fullmatch(r"[0-9a-f]{32}", path.parent.name):
        raise ValueError("image must be an owned upload")
    if path.parent.is_symlink() or root.is_symlink():
        raise ValueError("invalid upload directory")
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or not 0 < info.st_size <= 25 * 1024 * 1024:
            raise ValueError("invalid uploaded image")
        data = stream.read(25 * 1024 * 1024 + 1)
    mime = ("image/png" if data.startswith(b"\x89PNG\r\n\x1a\n") else
            "image/jpeg" if data.startswith(b"\xff\xd8\xff") else
            "image/gif" if data.startswith((b"GIF87a", b"GIF89a")) else
            "image/webp" if data.startswith(b"RIFF") and data[8:12] == b"WEBP" else None)
    if mime is None:
        raise ValueError("unsupported image encoding")
    return {"type": "image", "mimeType": mime, "data": base64.b64encode(data).decode()}


def native_input(blocks, model):
    if not isinstance(blocks, list) or not blocks or len(blocks) > 6:
        raise ValueError("invalid harness input")
    result, total = [], 0
    for block in blocks:
        kind = block.get("type")
        if kind == "text" and isinstance(block.get("text"), str):
            if len(block["text"].encode()) > 1024 * 1024:
                raise ValueError("harness text exceeds limit")
            total += len(block["text"].encode())
            result.append({"type": "text", "text": block["text"]})
        elif kind == "localImage" and "image" in model["inputModalities"]:
            value = image(block["path"])
            total += len(value["data"])
            result.append(value)
        else:
            raise ValueError("input modality unavailable")
    if total > 36 * 1024 * 1024 or not any(b.get("text", "").strip() for b in result):
        raise ValueError("invalid harness prompt")
    return result
