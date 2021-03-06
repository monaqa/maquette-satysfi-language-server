use std::{collections::HashMap, error::Error};

use log::{debug, error, info};
use maquette_satysfi_language_server::{Buffer, BufferCst, completion::get_completion_response, definition::get_definition_response};
use simplelog::*;

use lsp_types::{CompletionOptions, InitializeParams, OneOf, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url, notification::{DidChangeTextDocument, DidOpenTextDocument}, request::{Completion, GotoDefinition}};

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};

fn main() {
    let log_conf = ConfigBuilder::new()
        .set_time_to_local(true)
        .set_location_level(LevelFilter::Info)
        .build();
    WriteLogger::init(
        LevelFilter::Debug,
        log_conf,
        std::fs::File::create("test.log").unwrap(),
    )
    .unwrap();

    let result = sub();
    if let Err(e) = result {
        error!("{}", e);
        std::process::exit(1);
    }
}

fn sub() -> Result<(), Box<dyn Error + Sync + Send>> {
    // Note that  we must have our logging only write out to stderr.
    info!("starting generic LSP server");

    // Create the transport. Includes the stdio (stdin and stdout) versions but this could
    // also be implemented to use sockets or HTTP.
    let (connection, io_threads) = Connection::stdio();

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let server_capabilities = {
        let mut server_capabilities = ServerCapabilities::default();
        server_capabilities.definition_provider = Some(OneOf::Left(true));
        server_capabilities.text_document_sync =
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::Full));
        let mut compopt = CompletionOptions::default();
        compopt.trigger_characters = Some( vec!["\\".to_owned(), "+".to_owned() , "#".to_owned()]);
        server_capabilities.completion_provider = Some(compopt);
        serde_json::to_value(&server_capabilities).unwrap()
    };
    info!("server_capabilities: {:?}", server_capabilities);
    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(&connection, initialization_params)?;
    io_threads.join()?;

    // Shut down gracefully.
    info!("shutting down server");
    Ok(())
}

fn main_loop(
    connection: &Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();
    info!("starting example main loop");

    // let mut buffers = Buffers::default();
    let mut buffers: HashMap<Url, Buffer> = HashMap::new();

    for msg in &connection.receiver {
        info!("got msg: {:?}", msg);
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                info!("got request: {:?}", req);
                let method = &req.method;
                match method.as_str() {
                    "textDocument/completion" => {
                        let (id, params) = cast_req::<Completion>(req).unwrap();

                        let uri = &params.text_document_position.text_document.uri;
                        // let text = buffers.get(&uri);
                        let resp = buffers
                            .get(uri)
                            .and_then(|buf| get_completion_response(&buf, params));

                        if let Some(resp) = resp {
                            let result = serde_json::to_value(&resp).unwrap();
                            let resp = Response {
                                id,
                                result: Some(result),
                                error: None,
                            };
                            connection.sender.send(Message::Response(resp))?;
                            continue;
                        }
                    }
                    "textDocument/definition" => {
                        let (id, params) = cast_req::<GotoDefinition>(req).unwrap();

                        let uri = &params.text_document_position_params.text_document.uri;
                        let resp = buffers
                            .get(uri)
                            .and_then(|buf| get_definition_response(&buf, params))
                            ;

                        if let Some(resp) = resp {
                            let result = serde_json::to_value(&resp).unwrap();
                            let resp = Response {
                                id,
                                result: Some(result),
                                error: None,
                            };
                            connection.sender.send(Message::Response(resp))?;
                            continue;
                        }

                    }
                    _ => unreachable!(),
                }
                // ...
            }

            Message::Response(resp) => {
                info!("got response: {:?}", resp);
            }

            Message::Notification(not) => {
                info!("got notification: {:?}", not);
                let method = &not.method;
                match method.as_str() {
                    "textDocument/didChange" => {
                        let params = cast_notif::<DidChangeTextDocument>(not).unwrap();
                        let uri = params.text_document.uri;
                        if let Some(change) = params.content_changes.get(0) {
                            let text = change.text.clone();

                            let buf = Buffer::new(text);
                            debug!("buffer cst: {}", &buf.buf_cst);
                            debug!("buffer env: {:?}", buf.env);
                            if let Some(e) = buf.error.get(0) {
                                debug!("error: {:?}", e)
                            }
                            buffers.insert(uri, buf);
                        }
                    }
                    "textDocument/didOpen" => {
                        let params = cast_notif::<DidOpenTextDocument>(not).unwrap();
                        let uri = params.text_document.uri;
                        let text = params.text_document.text;

                        let buf = Buffer::new(text);
                        debug!("buffer cst: {}", buf.buf_cst);
                        debug!("buffer env: {:?}", buf.env);
                        if let Some(e) = buf.error.get(0) {
                            debug!("error: {:?}", e)
                        }
                        buffers.insert(uri, buf);
                    }
                    _ => (),
                }
            }
        }
    }
    Ok(())
}

fn cast_req<R>(req: Request) -> Result<(RequestId, R::Params), Request>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

fn cast_notif<R>(not: Notification) -> Result<R::Params, Notification>
where
    R: lsp_types::notification::Notification,
    R::Params: serde::de::DeserializeOwned,
{
    not.extract(R::METHOD)
}
