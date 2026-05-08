import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    // Determine the path to the Anvil binary
    // During development, we use `cargo run` from the project root
    const rootPath = workspace.workspaceFolders?.[0].uri.fsPath || "";
    const command = 'cargo';
    const args = ['run', '--manifest-path', path.join(rootPath, 'Cargo.toml'), '--', 'lsp'];

    // If it was a production build, it would just be:
    // const command = 'anvil';
    // const args = ['lsp'];

    const serverOptions: ServerOptions = {
        run: { command, args, transport: 0 }, // 0 = stdio
        debug: { command, args, transport: 0 }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'anvil' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher('**/.clientrc')
        }
    };

    client = new LanguageClient(
        'anvilLanguageServer',
        'Anvil Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
