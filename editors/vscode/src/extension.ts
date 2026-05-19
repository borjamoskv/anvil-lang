import * as fs from 'fs';
import * as path from 'path';
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    const executable = resolveServerExecutable(context);
    if (!executable) {
        window.showErrorMessage(
            'Anvil language server not found. Configure anvil.server.path, install anvil on PATH, or open the Anvil source checkout.'
        );
        return;
    }

    const serverOptions: ServerOptions = {
        run: executable,
        debug: executable
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

function resolveServerExecutable(context: ExtensionContext): Executable | undefined {
    const configuredPath = workspace.getConfiguration('anvil').get<string>('server.path');
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

function binaryExecutable(command: string): Executable {
    return {
        command,
        args: ['lsp'],
        transport: TransportKind.stdio
    };
}

function cargoExecutable(rootPath: string): Executable {
    return {
        command: 'cargo',
        args: ['run', '-j', '1', '--bin', 'anvil', '--manifest-path', path.join(rootPath, 'Cargo.toml'), '--', 'lsp'],
        transport: TransportKind.stdio,
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

function bundledServerPath(context: ExtensionContext): string | undefined {
    const candidate = path.join(
        context.extensionPath,
        'bin',
        `${process.platform}-${process.arch}`,
        serverBinaryName()
    );
    return isExecutableFile(candidate) ? candidate : undefined;
}

function findAnvilSourceRoot(context: ExtensionContext): string | undefined {
    const candidates = [
        path.resolve(context.extensionPath, '../..'),
        ...(workspace.workspaceFolders?.map(folder => folder.uri.fsPath) ?? [])
    ];

    return candidates.find(isAnvilSourceRoot);
}

function isAnvilSourceRoot(candidate: string): boolean {
    const manifest = path.join(candidate, 'Cargo.toml');
    const main = path.join(candidate, 'src', 'main.rs');
    if (!fs.existsSync(manifest) || !fs.existsSync(main)) {
        return false;
    }

    const manifestText = fs.readFileSync(manifest, 'utf8');
    return /^\s*name\s*=\s*"anvil"\s*$/m.test(manifestText);
}

function findOnPath(command: string): string | undefined {
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

function isExecutableFile(candidate: string): boolean {
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
    } catch {
        return false;
    }
}

function serverBinaryName(): string {
    return process.platform === 'win32' ? 'anvil.exe' : 'anvil';
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
