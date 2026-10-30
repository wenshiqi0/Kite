import base64
import hashlib
import io
import json
import os
from pathlib import Path
import socket
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import kite_codex as backend


class BackendTests(unittest.TestCase):
    def projection(self, file):
        output = io.BytesIO()
        backend.history_projection(str(file), output)
        marker, size, body = output.getvalue().split(b"\n", 2)
        self.assertEqual(marker, b"kite-codex-history-v2")
        size = int(size)
        self.assertLessEqual(len(output.getvalue()), 2097152)
        return body[:size], body[size:]

    def test_catalog_cas_prevents_archive_and_provider_from_being_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            first = backend.catalog_read()
            record = {"id": "a" * 32, "provider": "claude", "reference": None, "archived": True}
            written = backend.catalog_write(dict(first, sessions=[record]))
            self.assertEqual(written["revision"], 1)
            stale = backend.catalog_write(dict(first, sessions=[dict(record, archived=False, provider="codex")]))
            self.assertTrue(stale["conflict"])
            current = backend.catalog_read()["sessions"][0]
            self.assertTrue(current["archived"])
            self.assertEqual(current["provider"], "claude")
            backend.patch_session(record["id"], {"preview": "new preview"})
            self.assertTrue(backend.catalog_read()["sessions"][0]["archived"])
            self.assertEqual((Path(temporary) / ".local/share/kite/sessions.json").stat().st_mode & 0o777, 0o600)

    def test_large_inline_image_preserves_user_text_and_attachment_history(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file = Path(temporary) / "history.jsonl"
            original = {"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": 'literal {"image_url":"keep this text"} and attachment manifest'}, {"type": "input_image", "image_url": "data:image/png;base64," + "A" * (3 * 1024 * 1024)}]}}
            file.write_text(json.dumps(original) + "\n" + '{"type":"response_item","payload":{"type":"message","role":"assistant","content":[]}}\n')
            controls, recent = self.projection(file)
            self.assertEqual(json.loads(controls)["payload"]["content"], [])
            self.assertNotIn(b"image_url", controls)
            lines = recent.splitlines()
            self.assertEqual(len(lines), 2)
            projected = json.loads(lines[0])
            self.assertEqual(projected["payload"]["content"][0], original["payload"]["content"][0])
            self.assertEqual(projected["payload"]["content"][1]["image_url"], "")
            self.assertLess(len(recent), 1000)
            self.assertGreater(file.stat().st_size, 3 * 1024 * 1024)

    def test_old_questions_and_answers_survive_the_recent_history_window(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file = Path(temporary) / "history.jsonl"
            call = {"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"old-question","arguments":json.dumps({"questions":[{"title":"Choose","options":["A","B"]}]})}}
            accepted = {"type":"response_item","payload":{"type":"function_call_output","call_id":"old-question","output":"{\"accepted\":true}"}}
            noise = json.dumps({"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"unrelated","output":"x"*600}})+"\n"
            file.write_text(json.dumps(call)+"\n"+json.dumps(accepted)+"\n"+noise*5000)
            controls, recent = self.projection(file)
            self.assertEqual(len(controls.splitlines()), 2)
            self.assertIn(b"old-question", controls)
            self.assertNotIn(b"old-question", recent)
            self.assertEqual(self.projection(file)[0], controls)
            answer = {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<send_user_message_question_reply>\n[]\n</send_user_message_question_reply>"}]}}
            with file.open("a") as f: f.write(json.dumps(answer)+"\n")
            controls, _ = self.projection(file)
            self.assertEqual(len(controls.splitlines()), 3)
            cache = next((Path(temporary)/".local/state/kite/codex/history").glob("*.json"))
            self.assertEqual(cache.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads(cache.read_text())["offset"], file.stat().st_size)
            # An atomic replacement must invalidate the old index even at the same path.
            replacement = file.with_suffix(".replacement")
            replacement.write_text(noise)
            replacement.replace(file)
            self.assertEqual(self.projection(file)[0], b"")

    def test_partial_control_lines_are_retried_and_truncation_rebuilds_index(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file = Path(temporary)/"history.jsonl"
            call = json.dumps({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"partial","arguments":"{}"}})+"\n"
            file.write_text(call[:30])
            self.assertEqual(self.projection(file)[0], b"")
            with file.open("a") as f: f.write(call[30:])
            controls, _ = self.projection(file)
            self.assertEqual(len(controls.splitlines()),1)
            self.assertIn(b"partial",controls)
            file.write_text("{}\n")
            self.assertEqual(self.projection(file)[0],b"")

    def test_upload_is_private_verified_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            data = b"Kite attachment\x00\xff"
            digest = hashlib.sha256(data).hexdigest()
            with patch.object(backend.sys, "stdin", SimpleNamespace(buffer=io.BytesIO(data))):
                receipt = backend.upload("a" * 32, "sample.bin", len(data), digest)
            file = Path(receipt["path"])
            self.assertEqual(file.read_bytes(), data)
            self.assertEqual(file.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                backend.upload("a" * 32, "sample.bin", len(data), digest)
            with patch.object(backend.sys, "stdin", SimpleNamespace(buffer=io.BytesIO(data))):
                with self.assertRaises(ValueError):
                    backend.upload("b" * 32, "sample.bin", len(data), "0" * 64)
            self.assertFalse((file.parent.parent / ("b" * 32)).exists())
            for name in ("../escape", ".", "bad\nname", "x\\y"):
                with self.assertRaises(ValueError):
                    backend.upload("c" * 32, name, 1, digest)

    def test_unix_proxy_masks_requests_and_reassembles_fragments_with_ping(self):
        with tempfile.TemporaryDirectory(prefix="kite-wire-", dir="/tmp") as temporary:
            address = str(Path(temporary) / "server.sock")
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(address)
            listener.listen(1)
            errors = []
            received = []

            def server():
                try:
                    with listener.accept()[0] as conn:
                        conn.settimeout(5)
                        header = bytearray()
                        while not header.endswith(b"\r\n\r\n"):
                            header.extend(conn.recv(1))
                        key = next(line.split(b":", 1)[1].strip() for line in header.split(b"\r\n") if line.startswith(b"Sec-WebSocket-Key:"))
                        accept = base64.b64encode(hashlib.sha1(key + b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest())
                        conn.sendall(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: " + accept + b"\r\n\r\n")
                        def exact(n):
                            data = b""
                            while len(data) < n:
                                part = conn.recv(n-len(data))
                                if not part:
                                    raise EOFError()
                                data += part
                            return data
                        a, b = exact(2)
                        self.assertEqual(a, 0x81)
                        self.assertTrue(b & 0x80)
                        mask = exact(4)
                        data = exact(b & 127)
                        received.append(json.loads(bytes(v ^ mask[i % 4] for i, v in enumerate(data))))
                        conn.sendall(b'\x01\x08{"id":1,' + b'\x89\x01p' + b'\x80\x0c"result":{}}')
                        # Consume the masked pong before closing.
                        a, b = exact(2)
                        self.assertEqual(a, 0x8a)
                        mask = exact(4)
                        self.assertEqual(bytes([exact(1)[0] ^ mask[0]]), b"p")
                        conn.sendall(b"\x88\x00")
                        exact(6)
                except BaseException as error:
                    errors.append(error)

            worker = threading.Thread(target=server, daemon=True)
            worker.start()
            read, write = os.pipe()
            os.write(write, b'{"id":1,"method":"initialize"}\n')
            output = io.BytesIO()
            try:
                with patch.object(backend, "read_registry", return_value={"socket": address}), patch.object(backend.sys, "stdin", SimpleNamespace(fileno=lambda: read)), patch.object(backend.sys, "stdout", SimpleNamespace(buffer=output)):
                    backend.proxy("w1:p1")
            finally:
                os.close(read); os.close(write); listener.close()
            worker.join(5)
            if errors:
                raise errors[0]
            self.assertFalse(worker.is_alive())
            self.assertEqual(received, [{"id": 1, "method": "initialize"}])
            self.assertEqual(json.loads(output.getvalue()), {"id": 1, "result": {}})


if __name__ == "__main__":
    unittest.main()
