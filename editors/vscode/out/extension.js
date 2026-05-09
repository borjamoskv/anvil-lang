"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const path = require("path");
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    // Determine the path to the Anvil binary
    // During development, we use `cargo run` from the project root
    const rootPath = vscode_1.workspace.workspaceFolders?.[0].uri.fsPath || "";
    const command = 'cargo';
    const args = ['run', '--manifest-path', path.join(rootPath, 'Cargo.toml'), '--', 'lsp'];
    // If it was a production build, it would just be:
    // const command = 'anvil';
    // const args = ['lsp'];
    const serverOptions = {
        run: { command, args, transport: 0 }, // 0 = stdio
        debug: { command, args, transport: 0 }
    };
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'anvil' }],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher('**/.clientrc')
        }
    };
    client = new node_1.LanguageClient('anvilLanguageServer', 'Anvil Language Server', serverOptions, clientOptions);
    client.start();
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
//# sourceMappingURL=extension.js.map