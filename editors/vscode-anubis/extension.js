const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

/** @type {import('vscode-languageclient/node').LanguageClient | undefined} */
let client;

/**
 * @param {import('vscode').ExtensionContext} context
 */
function activate(context) {
  const vscode = require('vscode');
  const config = vscode.workspace.getConfiguration('anubis');
  const command = config.get('lspPath') || 'anubis';

  /** @type {import('vscode-languageclient/node').ServerOptions} */
  const serverOptions = {
    command,
    args: ['lsp'],
    transport: TransportKind.stdio,
  };

  /** @type {import('vscode-languageclient/node').LanguageClientOptions} */
  const clientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'anubis' },
      { scheme: 'untitled', language: 'anubis' },
    ],
    synchronize: {
      // re-analyze on save; didChange still flows from full document sync
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{anb,anubis,anub}'),
    },
  };

  client = new LanguageClient(
    'anubis',
    'Anubis Language Server',
    serverOptions,
    clientOptions
  );

  // v9: start() returns a Promise; keep disposable for deactivate.
  context.subscriptions.push({
    dispose: () => {
      if (client) {
        return client.stop();
      }
    },
  });

  client.start().then(
    () => {
      // ready
    },
    (err) => {
      vscode.window.showErrorMessage(
        `Anubis LSP failed to start (is \`${command} lsp\` on PATH?): ${err}`
      );
    }
  );
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

module.exports = { activate, deactivate };
