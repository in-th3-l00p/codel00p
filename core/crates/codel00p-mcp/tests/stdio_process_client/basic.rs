use super::*;

#[tokio::test]
async fn stdio_client_sends_json_rpc_requests_to_process_and_reads_responses() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            "read line; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo input.\",\"inputSchema\":{\"type\":\"object\"}}]}}'",
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let response = client
        .request("tools/list", json!({}))
        .await
        .expect("tools/list response");

    assert_eq!(
        response,
        json!({
            "tools": [
                {
                    "name": "echo",
                    "description": "Echo input.",
                    "inputSchema": { "type": "object" }
                }
            ]
        })
    );
}

#[tokio::test]
async fn stdio_client_lists_and_calls_mcp_tools() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            "read first; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo input.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}}}}]}}'; read second; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"hello\"}],\"isError\":false}}'",
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let tools = client.list_tools().await.expect("list tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].server_id(), "fake");
    assert_eq!(tools[0].tool_name(), "echo");
    assert_eq!(tools[0].description(), "Echo input.");
    assert_eq!(
        tools[0].input_schema()["properties"]["text"]["type"],
        "string"
    );

    let output = client
        .call_tool(McpToolCall::new("fake", "echo", json!({ "text": "hello" })))
        .await
        .expect("call tool");

    assert_eq!(
        output.content(),
        &json!({
            "content": [
                { "type": "text", "text": "hello" }
            ],
            "isError": false
        })
    );
}

#[tokio::test]
async fn stdio_client_lists_and_gets_mcp_prompts() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read list; case "$list" in *'"method":"prompts/list"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"prompts":[{"name":"review_pr","description":"Review a pull request.","arguments":[{"name":"diff","description":"Unified diff","required":true}]}]}}'; read get; case "$get" in *'"method":"prompts/get"'*) ;; *) exit 12;; esac; case "$get" in *'"name":"review_pr"'*) ;; *) exit 13;; esac; case "$get" in *'"diff":"patch"'*) ;; *) exit 14;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"description":"Review a pull request.","messages":[{"role":"user","content":{"type":"text","text":"Review this patch."}}]}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let prompts = client.list_prompts().await.expect("list prompts");
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].server_id(), "fake");
    assert_eq!(prompts[0].name(), "review_pr");
    assert_eq!(prompts[0].description(), Some("Review a pull request."));
    assert_eq!(prompts[0].arguments()[0].name(), "diff");
    assert!(prompts[0].arguments()[0].required());

    let prompt = client
        .get_prompt("review_pr", json!({ "diff": "patch" }))
        .await
        .expect("get prompt");
    assert_eq!(prompt.description(), Some("Review a pull request."));
    assert_eq!(
        prompt.messages(),
        &[McpPromptMessage::text("user", "Review this patch.")]
    );
}

#[tokio::test]
async fn stdio_client_reads_resources_templates_and_sets_logging_level() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r##"read templates; case "$templates" in *'"method":"resources/templates/list"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"resourceTemplates":[{"uriTemplate":"file:///{path}","name":"workspace file","description":"Read a workspace file.","mimeType":"text/plain"}]}}'; read resource; case "$resource" in *'"method":"resources/read"'*'"uri":"file:///README.md"'*) ;; *) exit 12;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"contents":[{"uri":"file:///README.md","mimeType":"text/markdown","text":"# codel00p"}]}}'; read logging; case "$logging" in *'"method":"logging/setLevel"'*'"level":"warning"'*) ;; *) exit 13;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{}}'"##,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let templates = client
        .list_resource_templates()
        .await
        .expect("list resource templates");
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].server_id(), "fake");
    assert_eq!(templates[0].uri_template(), "file:///{path}");
    assert_eq!(templates[0].name(), "workspace file");
    assert_eq!(templates[0].description(), Some("Read a workspace file."));
    assert_eq!(templates[0].mime_type(), Some("text/plain"));

    let resource = client
        .read_resource("file:///README.md")
        .await
        .expect("read resource");
    assert_eq!(resource.contents().len(), 1);
    assert_eq!(resource.contents()[0].uri(), "file:///README.md");
    assert_eq!(resource.contents()[0].mime_type(), Some("text/markdown"));
    assert_eq!(resource.contents()[0].text(), Some("# codel00p"));

    client
        .set_logging_level("warning")
        .await
        .expect("set logging level");
}

#[tokio::test]
async fn stdio_client_paginates_list_methods_until_cursor_is_absent() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r##"read tools_first
case "$tools_first" in *'"method":"tools/list"'*) ;; *) exit 11;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"search","description":"Search memory.","inputSchema":{"type":"object"}}],"nextCursor":"tools-2"}}'
read tools_second
case "$tools_second" in *'"method":"tools/list"'*'"cursor":"tools-2"'*) ;; *) exit 12;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"open","description":"Open resource.","inputSchema":{"type":"object"}}]}}'
read resources_first
case "$resources_first" in *'"method":"resources/list"'*) ;; *) exit 13;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"resources":[{"uri":"codel00p://memory/one","name":"Memory one","mimeType":"text/plain"}],"nextCursor":"resources-2"}}'
read resources_second
case "$resources_second" in *'"method":"resources/list"'*'"cursor":"resources-2"'*) ;; *) exit 14;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"resources":[{"uri":"codel00p://memory/two","name":"Memory two","mimeType":"text/plain"}]}}'
read templates_first
case "$templates_first" in *'"method":"resources/templates/list"'*) ;; *) exit 15;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":5,"result":{"resourceTemplates":[{"uriTemplate":"file:///{path}","name":"workspace file","mimeType":"text/plain"}],"nextCursor":"templates-2"}}'
read templates_second
case "$templates_second" in *'"method":"resources/templates/list"'*'"cursor":"templates-2"'*) ;; *) exit 16;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":6,"result":{"resourceTemplates":[{"uriTemplate":"db:///{table}/{id}","name":"database row","mimeType":"application/json"}]}}'
read prompts_first
case "$prompts_first" in *'"method":"prompts/list"'*) ;; *) exit 17;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":7,"result":{"prompts":[{"name":"review","description":"Review code."}],"nextCursor":"prompts-2"}}'
read prompts_second
case "$prompts_second" in *'"method":"prompts/list"'*'"cursor":"prompts-2"'*) ;; *) exit 18;; esac
printf '%s\n' '{"jsonrpc":"2.0","id":8,"result":{"prompts":[{"name":"summarize","description":"Summarize memory."}]}}'"##,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let tools = client.list_tools().await.expect("list tools");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].tool_name(), "search");
    assert_eq!(tools[1].tool_name(), "open");

    let resources = client.list_resources().await.expect("list resources");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].uri(), "codel00p://memory/one");
    assert_eq!(resources[1].uri(), "codel00p://memory/two");

    let templates = client
        .list_resource_templates()
        .await
        .expect("list resource templates");
    assert_eq!(templates.len(), 2);
    assert_eq!(templates[0].uri_template(), "file:///{path}");
    assert_eq!(templates[1].uri_template(), "db:///{table}/{id}");

    let prompts = client.list_prompts().await.expect("list prompts");
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0].name(), "review");
    assert_eq!(prompts[1].name(), "summarize");
}
