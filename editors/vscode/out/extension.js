"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const fs = require("fs");
const path = require("path");
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const executable = resolveServerExecutable(context);
    if (!executable) {
        vscode_1.window.showErrorMessage('Anvil language server not found. Configure anvil.server.path, install anvil on PATH, or open the Anvil source checkout.');
        return;
    }
    const serverOptions = {
        run: executable,
        debug: executable
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
function resolveServerExecutable(context) {
    const configuredPath = vscode_1.workspace.getConfiguration('anvil').get('server.path');
    if (configuredPath && configuredPath.trim().length > 0) {
        return binaryExecutable(configuredPath.trim());
    }
    const bundled = bundledServerPath(context);
    if (bundled) {
        return binaryExecutable(bundled);
    }
    const pathBinary = findOnPath(serverBinaryName());
    if (pathBinary) {
        return binaryExecutable(pathBinary);
    }
    const sourceRoot = findAnvilSourceRoot(context);
    if (sourceRoot) {
        return cargoExecutable(sourceRoot);
    }
    return undefined;
}
function binaryExecutable(command) {
    return {
        command,
        args: ['lsp'],
        transport: node_1.TransportKind.stdio
    };
}
function cargoExecutable(rootPath) {
    return {
        command: 'cargo',
        args: ['run', '-j', '1', '--bin', 'anvil', '--manifest-path', path.join(rootPath, 'Cargo.toml'), '--', 'lsp'],
        transport: node_1.TransportKind.stdio,
        options: {
            cwd: rootPath,
            env: {
                ...process.env,
                CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '1',
                CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL ?? '0',
                CARGO_PROFILE_DEV_DEBUG: process.env.CARGO_PROFILE_DEV_DEBUG ?? '0'
            }
        }
    };
}
function bundledServerPath(context) {
    const candidate = path.join(context.extensionPath, 'bin', `${process.platform}-${process.arch}`, serverBinaryName());
    return isExecutableFile(candidate) ? candidate : undefined;
}
function findAnvilSourceRoot(context) {
    const candidates = [
        path.resolve(context.extensionPath, '../..'),
        ...(vscode_1.workspace.workspaceFolders?.map(folder => folder.uri.fsPath) ?? [])
    ];
    return candidates.find(isAnvilSourceRoot);
}
function isAnvilSourceRoot(candidate) {
    const manifest = path.join(candidate, 'Cargo.toml');
    const main = path.join(candidate, 'src', 'main.rs');
    if (!fs.existsSync(manifest) || !fs.existsSync(main)) {
        return false;
    }
    const manifestText = fs.readFileSync(manifest, 'utf8');
    return /^\s*name\s*=\s*"anvil"\s*$/m.test(manifestText);
}
function findOnPath(command) {
    const pathEnv = process.env.PATH;
    if (!pathEnv) {
        return undefined;
    }
    const hasExtension = path.extname(command).length > 0;
    const extensions = process.platform === 'win32' && !hasExtension
        ? (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM').split(';')
        : [''];
    for (const dir of pathEnv.split(path.delimiter)) {
        for (const ext of extensions) {
            const candidate = path.join(dir, `${command}${ext}`);
            if (isExecutableFile(candidate)) {
                return candidate;
            }
        }
    }
    return undefined;
}
function isExecutableFile(candidate) {
    try {
        const stat = fs.statSync(candidate);
        if (!stat.isFile()) {
            return false;
        }
        if (process.platform === 'win32') {
            return true;
        }
        fs.accessSync(candidate, fs.constants.X_OK);
        return true;
    }
    catch {
        return false;
    }
}
function serverBinaryName() {
    return process.platform === 'win32' ? 'anvil.exe' : 'anvil';
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
//# sourceMappingURL=extension.js.map