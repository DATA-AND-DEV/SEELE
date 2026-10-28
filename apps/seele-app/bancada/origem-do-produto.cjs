// A origem da bancada segue o mesmo contrato Rust usado pelo WASM.
const fs = require('node:fs');
const path = require('node:path');
const fonte = fs.readFileSync(path.resolve(__dirname, '../../../crates/seele-proto/src/uri.rs'), 'utf8');
const host = fonte.match(/pub const ORIGEM_DO_NAVEGADOR: &str = "([^"]+)";/)?.[1];
if (!host) throw new Error('Constante Rust da origem do navegador não encontrada');
module.exports = { origemDoProduto: `https://${host}` };
