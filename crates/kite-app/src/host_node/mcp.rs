use super::{API, Error, NodeRequest, Result, json_line, rpc, version};
use super::{desktop, docs};
use base64::Engine;
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

fn tools() -> Value {
    let mut value = json!({"tools":[
        {"name":"kite_host_status","description":"Read this Kite host node and its configured runtime services.","inputSchema":{"type":"object","additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"kite_sessions_list","description":"Read saved provider session references on this host; no model turn is started.","inputSchema":{"type":"object","additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"kite_docs","description":"Read concise Kite guides once, then reuse retained bodies. Desktop covers common actions; known topic revisions omit unchanged bodies.","inputSchema":{"type":"object","properties":{"topics":{"type":"array","items":{"type":"string"},"maxItems":4},"known":{"type":"object","additionalProperties":{"type":"string"}}},"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"kite_desktop","description":"Operate the configured X11 desktop. Read kite_docs desktop once for parameters. Screenshot returns a native image; input changes the real desktop.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["status","windows","screenshot","move","click","scroll","key","type","launch"]},"params":{"type":"object"}},"required":["action"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":true,"openWorldHint":true}}
    ]});
    if cfg!(target_os = "macos") {
        value["tools"]
            .as_array_mut()
            .unwrap()
            .retain(|tool| tool["name"] != "kite_desktop");
    }
    value
}
pub async fn response(socket: &Path, request: &Value) -> Result<Option<Value>> {
    let Some(id) = request.get("id") else {
        return Ok(None);
    };
    if request["jsonrpc"] != "2.0" {
        return Ok(Some(
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid Request"}}),
        ));
    }
    let method = request["method"].as_str().unwrap_or("");
    let result: std::result::Result<Value, (i32, String)> = match method {
        "initialize" => Ok(
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":if cfg!(target_os = "macos") {"kite-macos-host"} else {"kite-linux-host"},"version":version()},"instructions":format!("Kite API {API}. Read kite_docs for available host and session operations; reuse retained bodies. Desktop actions are available only on Linux X11 hosts.")}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or("");
            let arguments = &request["params"]["arguments"];
            if name == "kite_docs" {
                let args = if arguments.is_null() {
                    json!({})
                } else {
                    arguments.clone()
                };
                let result = serde_json::from_value::<docs::Request>(args)
                    .map_err(|_| (-32602, "Invalid docs arguments".into()))
                    .and_then(|args| docs::read(args).map_err(|e| (-32602, e.to_string())));
                result.map(|documents|json!({"content":[{"type":"text","text":docs::text(&documents)}],"isError":false}))
            } else if name == "kite_desktop" {
                match desktop_arguments(arguments) {
                    Err(error) => Err((-32602, error.to_string())),
                    Ok(params) => {
                        let mut request = NodeRequest::new("desktop");
                        request.params = params;
                        Ok(match rpc(socket, request).await {
                            Ok(value) => match desktop::image(socket, &value) {
                                Ok(png) => {
                                    let mut content = vec![
                                        json!({"type":"text","text":serde_json::to_string(&value)?}),
                                    ];
                                    if let Some(png) = png {
                                        content.push(json!({"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png)}));
                                    }
                                    json!({"content":content,"structuredContent":value,"isError":false})
                                }
                                Err(error) => tool_error(error),
                            },
                            Err(error) => tool_error(error),
                        })
                    }
                }
            } else {
                let control = match name {
                    "kite_host_status" => "status",
                    "kite_sessions_list" => "sessions",
                    _ => "",
                };
                if control.is_empty()
                    || !(arguments.is_null() || arguments.as_object().is_some_and(|a| a.is_empty()))
                {
                    Err((-32602, "Unknown tool or nonempty arguments".into()))
                } else {
                    let result = rpc(socket, NodeRequest::new(control)).await;
                    Ok(match result {
                        Ok(value) => {
                            json!({"content":[{"type":"text","text":serde_json::to_string(&value)?}],"structuredContent":value,"isError":false})
                        }
                        Err(error) => {
                            json!({"content":[{"type":"text","text":error.to_string()}],"isError":true})
                        }
                    })
                }
            }
        }
        _ => Err((-32601, "Method not found".into())),
    };
    Ok(Some(match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    }))
}
fn tool_error(error: Error) -> Value {
    json!({"content":[{"type":"text","text":error.to_string()}],"isError":true})
}
fn desktop_arguments(arguments: &Value) -> Result<Value> {
    let args = arguments
        .as_object()
        .ok_or(Error::Config("desktop arguments must be an object"))?;
    if args.keys().any(|key| key != "action" && key != "params") {
        return Err(Error::Config("unknown desktop argument"));
    }
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .ok_or(Error::Config("desktop action is required"))?;
    let mut params = match args.get("params") {
        None => serde_json::Map::new(),
        Some(Value::Object(params)) => params.clone(),
        _ => return Err(Error::Config("desktop params must be an object")),
    };
    if params.contains_key("action") {
        return Err(Error::Config("desktop action must not be overridden"));
    }
    params.insert("action".into(), json!(action));
    serde_json::to_value(desktop::parse(Value::Object(params))?).map_err(Error::from)
}
pub async fn serve(socket: &Path) -> Result<()> {
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        if input.fill_buf().await?.is_empty() {
            return Ok(());
        }
        match super::read_json_bounded(&mut input, 64 * 1024).await {
            Ok(request) => {
                if let Some(result) = response(socket, &request).await? {
                    json_line(&mut output, &result).await?;
                }
            }
            Err(Error::Json(_)) => json_line(
                &mut output,
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
            )
            .await?,
            Err(error) => return Err(error),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn initialization_and_discovery_are_valid_without_a_running_node() {
        let socket = Path::new("/missing/kite.sock");
        let initialized = response(
            socket,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
        let listed = response(
            socket,
            &json!({"jsonrpc":"2.0","id":"tools","method":"tools/list"}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            listed["result"]["tools"].as_array().unwrap().len(),
            if cfg!(target_os = "macos") { 3 } else { 4 }
        );
        assert!(
            response(
                socket,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .await
            .unwrap()
            .is_none()
        );
    }
    #[tokio::test]
    async fn unavailable_node_is_a_tool_error_not_a_fabricated_status() {
        let result=response(Path::new("/missing/kite.sock"),&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kite_host_status","arguments":{}}})).await.unwrap().unwrap();
        assert_eq!(result["result"]["isError"], true);
    }
    #[tokio::test]
    async fn tool_arguments_cannot_become_arbitrary_backend_commands() {
        let result=response(Path::new("/missing/kite.sock"),&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kite_host_status","arguments":{"command":"rm"}}})).await.unwrap().unwrap();
        assert_eq!(result["error"]["code"], -32602);
    }
    #[tokio::test]
    async fn docs_work_offline_and_a_new_context_is_not_suppressed() {
        let socket = Path::new("/missing/owned-docs.sock");
        let call = |known: Value| json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kite_docs","arguments":{"topics":["desktop"],"known":known}}});
        let first = response(socket, &call(json!({}))).await.unwrap().unwrap();
        let text = first["result"]["content"][0]["text"].as_str().unwrap();
        let revision = text
            .split("revision=")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        let cached = response(socket, &call(json!({"desktop":revision})))
            .await
            .unwrap()
            .unwrap();
        assert!(
            cached["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .len()
                < text.len() / 5
        );
        let fresh = response(socket, &call(json!({}))).await.unwrap().unwrap();
        assert_eq!(first["result"], fresh["result"]);
        assert!(
            first["result"].get("structuredContent").is_none(),
            "do not duplicate the document body"
        );
    }
}
