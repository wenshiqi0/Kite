use super::{API, Error, NodeRequest, Result, json_line, read_json, rpc, version};
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

fn tools() -> Value {
    json!({"tools":[
        {"name":"kite_host_status","description":"Read this Kite host node and its configured runtime services.","inputSchema":{"type":"object","additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"kite_sessions_list","description":"Read saved provider session references on this host; no model turn is started.","inputSchema":{"type":"object","additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}
    ]})
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
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"kite-linux-host","version":version()},"instructions":format!("Host-local Kite Node API {API}; query tools do not submit model input.")}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or("");
            let arguments = &request["params"]["arguments"];
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
        _ => Err((-32601, "Method not found".into())),
    };
    Ok(Some(match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    }))
}
pub async fn serve(socket: &Path) -> Result<()> {
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        if input.fill_buf().await?.is_empty() {
            return Ok(());
        }
        match read_json(&mut input).await {
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
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 2);
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
}
