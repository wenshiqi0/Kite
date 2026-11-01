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
    def page(self, file, before=None, after=None):
        output = io.BytesIO()
        backend.history_projection(str(file), output, before, after)
        marker, size, body = output.getvalue().split(b"\n", 2)
        self.assertEqual(marker, b"kite-codex-history-v2")
        size = int(size)
        self.assertLessEqual(len(output.getvalue()), 4*1024*1024)
        meta, records = body[size:].split(b"\n", 1)
        page = json.loads(meta)
        self.assertEqual(page["type"], "kite_history_page")
        return page, body[:size], records

    def projection(self, file):
        _, controls, records = self.page(file)
        return controls, records

    def test_ten_thousand_messages_keep_all_records_and_compact_only_redundant_controls(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path,"home",return_value=Path(temporary)):
            file=Path(temporary)/"history.jsonl"
            with file.open("w") as stream:
                for i in range(10000):
                    stream.write(json.dumps({"id":f"m-{i}","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"same repeated body"}]}})+"\n")
            page,controls,body=self.page(file)
            found=[json.loads(line)["id"] for line in body.splitlines()]
            while page["has_older"]:
                page,_,body=self.page(file,before=page["start"])
                found=[json.loads(line)["id"] for line in body.splitlines()]+found
            self.assertEqual(found,[f"m-{i}" for i in range(10000)])
            self.assertEqual(len(controls.splitlines()),1)
            self.assertIn("_kite_offset",json.loads(body.splitlines()[0]))

    def test_large_text_is_preserved_and_oversized_records_are_not_silently_skipped(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path,"home",return_value=Path(temporary)):
            file=Path(temporary)/"history.jsonl"
            text="x"*(3*1024*1024)
            row={"id":"large","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":text}]}}
            file.write_text(json.dumps(row)+"\n")
            _,_,body=self.page(file)
            self.assertEqual(json.loads(body)["payload"]["content"][0]["text"],text)
            row["payload"]["content"][0]["text"]="x"*(5*1024*1024)
            file.write_text(json.dumps(row)+"\n")
            with self.assertRaisesRegex(ValueError,"transport limit"):self.page(file)

    def test_long_history_can_be_read_backwards_and_forwards_without_gaps(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file = Path(temporary)/"history.jsonl"
            records = [{"id":f"message-{i}","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":f"历史消息 {i}"}]}} for i in range(650)]
            noise = '{"type":"event_msg","payload":{"type":"task_started"}}\n'
            file.write_text("".join(json.dumps(r,ensure_ascii=False)+"\n"+noise*8 for r in records))
            latest, _, body = self.page(file)
            reverse = [json.loads(line)["id"] for line in body.splitlines()]
            page = latest
            while page["has_older"]:
                page, _, body = self.page(file, before=page["start"])
                reverse = [json.loads(line)["id"] for line in body.splitlines()] + reverse
            self.assertEqual(reverse, [r["id"] for r in records])
            forward = [json.loads(line)["id"] for line in body.splitlines()]
            while True:
                page, _, body = self.page(file, after=page["end"])
                forward.extend(json.loads(line)["id"] for line in body.splitlines())
                if not page["has_newer"]:break
            # The last backwards page can contain only source metadata.
            self.assertEqual(forward,[r["id"] for r in records])
            appended={"id":"appended","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"追加"}]}}
            with file.open("a") as stream:stream.write(json.dumps(appended)+"\n")
            _,_,body=self.page(file,after=latest["end"])
            self.assertEqual([json.loads(line)["id"] for line in body.splitlines()],["appended"])

    def test_history_cursors_reject_replacement_truncation_and_malformed_values(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file=Path(temporary)/"history.jsonl"
            row={"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"long enough"}]}}
            file.write_text(json.dumps(row)+"\n")
            page,_,_=self.page(file)
            for invalid in ("", "1:2:3", "../../etc/passwd", "1:2:-1:3"):
                with self.assertRaises(ValueError):self.page(file,before=invalid)
            file.write_text("{}\n")
            with self.assertRaisesRegex(ValueError,"source changed"):self.page(file,before=page["start"])
            replacement=file.with_suffix(".new");replacement.write_text(json.dumps(row)+"\n");replacement.replace(file)
            with self.assertRaisesRegex(ValueError,"source changed"):self.page(file,after=page["end"])

    def test_partial_display_record_is_not_published_until_complete(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            file=Path(temporary)/"history.jsonl"
            row={"id":"complete","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"ready"}]}}
            pending=dict(row,id="pending")
            file.write_text(json.dumps(row)+"\n"+json.dumps(pending))
            page,_,body=self.page(file)
            self.assertEqual([json.loads(line)["id"] for line in body.splitlines()],["complete"])
            with file.open("a") as stream:stream.write("\n")
            _,_,body=self.page(file,after=page["end"])
            self.assertEqual([json.loads(line)["id"] for line in body.splitlines()],["pending"])

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

    def test_notification_scan_excludes_archived_pending_and_unowned_sessions(self):
        reference=dict(agent="codex",kind="id",value="01234567-0123-0123-0123-0123456789ab")
        records=[dict(id="active",title="project",reference=reference),dict(id="archived",title="old",reference=reference,archived=True),dict(id="pending",title="pending",reference=None,terminal_id="pending"),dict(id="missing",title="missing",reference=None,terminal_id="missing")]
        agent=dict(reference=reference,terminal_id="terminal",pane_id="pane",agent_status="working",state_change_seq=2)
        with patch.object(backend,"api",return_value=dict(agents=[agent,dict(terminal_id="pending",launch_pending=True)])) as api,patch.object(backend,"catalog_read",return_value=dict(sessions=records)),patch.object(backend,"same_reference",side_effect=lambda a,r:a.get("reference") is not None and a.get("reference")==r.get("reference")):
            result=json.loads(backend.notification_facts("herdr"))
        self.assertEqual([row["session_id"] for row in result["facts"]],["active"])
        self.assertTrue(result["facts"][0]["lifecycle"])
        api.assert_called_once_with("herdr","agent.list",{})

    def test_control_index_retains_only_latest_terminal_fact_without_model_message_body(self):
        with tempfile.TemporaryDirectory() as temporary,patch.object(backend.Path,"home",return_value=Path(temporary)):
            file=Path(temporary)/"thread.jsonl"
            file.write_text("".join(json.dumps(dict(type="event_msg",payload=dict(type="task_complete",turn_id=str(i),last_agent_message="private answer")))+"\n" for i in range(400)))
            records=backend.question_records(file)
            self.assertEqual(len(records.splitlines()),1)
            self.assertEqual(json.loads(records)["payload"],dict(type="task_complete",turn_id="399"))
            self.assertNotIn(b"private answer",records)
            self.assertEqual(backend.question_records(file),records)


if __name__ == "__main__":
    unittest.main()

class GeneratedImageTests(unittest.TestCase):
    page = BackendTests.page
    def test_large_generated_images_are_small_manifests_and_download_exactly(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            # Larger than the transcript limit; no image data may enter its pages.
            data = b"\x89PNG\r\n\x1a\n" + b"x" * (5 * 1024 * 1024)
            row = {"type":"response_item", "payload":{"type":"image_generation_call", "id":"ig-test", "status":"completed", "result":base64.b64encode(data).decode()}}
            source = Path(temporary) / "history.jsonl"
            source.write_text(json.dumps(row) + "\n")
            _, controls, body = self.page(source)
            self.assertLess(len(body), 2048)
            self.assertNotIn(b"result", body)
            self.assertNotIn(b"_kite_images", controls)
            image = json.loads(body)["payload"]["_kite_images"][0]
            self.assertEqual(image["size"], len(data))
            self.assertEqual(image["id"], hashlib.sha256(data).hexdigest())
            output = io.BytesIO()
            backend.read_generated_image(image["id"], output)
            self.assertEqual(output.getvalue(), data)
            self.assertEqual(source.read_text(), json.dumps(row) + "\n")
            self.assertEqual(self.page(source)[2], body)

    def test_file_backed_completion_is_projected_and_partial_results_are_deferred(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            image = Path(temporary) / "result.webp"
            data = b"RIFF1234WEBPtest"
            image.write_bytes(data)
            source = Path(temporary) / "history.jsonl"
            row = {"type":"event_msg", "payload":{"type":"image_generation_end", "call_id":"ig-file", "status":"completed", "file_id":"file-1", "saved_path":str(image)}}
            source.write_text(json.dumps(row))
            self.assertFalse(self.page(source)[2])
            source.write_text(json.dumps(row) + "\n")
            payload = json.loads(self.page(source)[2])["payload"]
            self.assertEqual(payload["id"], "ig-file")
            self.assertEqual(payload["type"], "image_generation_call")
            self.assertEqual(payload["_kite_images"][0]["mime"], "image/webp")
            output = io.BytesIO()
            backend.read_generated_image(payload["_kite_images"][0]["id"], output)
            self.assertEqual(output.getvalue(), data)

    def test_invalid_results_do_not_break_chat_and_image_reads_reject_tampering(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(backend.Path, "home", return_value=Path(temporary)):
            source = Path(temporary) / "history.jsonl"
            for result in ("bad base64!", base64.b64encode(b"not an image").decode()):
                source.write_text(json.dumps({"type":"response_item", "payload":{"type":"image_generation_call", "id":"invalid", "result":result, "status":"completed"}}) + "\n")
                payload = json.loads(self.page(source)[2])["payload"]
                self.assertEqual(payload["_kite_images"], [])
                self.assertTrue(payload["_kite_image_error"])
            for identifier in ("../secret", "a" * 63, "A" * 64):
                with self.assertRaises(ValueError): backend.read_generated_image(identifier, io.BytesIO())
            image = backend.store_generated_bytes(b"\xff\xd8\xfftest")
            (backend.image_store() / image["id"]).write_bytes(b"changed")
            with self.assertRaises(ValueError): backend.read_generated_image(image["id"], io.BytesIO())
