using System.Text.Json;
using Kite;

var mirror = new ConversationMirror();
var vault = new SecretVault(Path.Combine(Path.GetTempPath(), "kite-missing-secrets-" + Guid.NewGuid().ToString("N")));
if(vault.Read("ssh-ed25519") is not null)throw new InvalidOperationException("missing secret directory must represent first launch");
void Apply(string json) { using var document = JsonDocument.Parse(json); mirror.Apply(document.RootElement); }
void Check(bool condition, string message) { if(!condition)throw new InvalidOperationException(message); }
Apply("""{"context":"host-one/session-one","revision":3,"reset":true,"edits":[{"at":0,"remove":0,"items":[{"id":"one"},{"id":"two"}]}]}""");
Apply("""{"context":"host-one/session-one","revision":4,"reset":false,"edits":[{"at":1,"remove":1,"items":[{"id":"edited"}]}]}""");
Check(mirror.Items.Count == 2 && mirror.Items[1].Text("id") == "edited", "replace streamed item");
Apply("""{"context":"host-one/session-one","revision":1,"reset":true,"edits":[]}""");
Check(mirror.Items.Count == 2, "reject stale delta");
try { Apply("""{"context":"host-two/session-two","revision":5,"reset":false,"edits":[]}"""); throw new Exception("context mismatch accepted"); } catch(InvalidDataException) { }
try { Apply("""{"context":"host-one/session-one","revision":5,"reset":false,"edits":[{"at":0,"remove":1,"items":[]},{"at":9,"remove":1,"items":[]}]}"""); throw new Exception("invalid transaction accepted"); } catch(InvalidDataException) { }
Check(mirror.Items.Count == 2 && mirror.Revision == 4, "invalid edits are atomic");
Apply("""{"context":"host-two/session-two","revision":1,"reset":true,"edits":[]}""");
Check(mirror.Items.Count == 0, "clear previous host's conversation");
var viewport = Viewport.Fit(1000,1000,1920,1080);
Check(viewport.Point(500,100,1920,1080) is null, "ignore letterbox input");
Check(viewport.Point(500,500,1920,1080) == (960,540), "source-coordinate mapping");
Check(Viewport.Fit(0,0,0,0).Point(0,0,0,0) is null, "empty viewport");
Console.WriteLine("PASS: conversation delta transactions, target isolation, viewport geometry");
