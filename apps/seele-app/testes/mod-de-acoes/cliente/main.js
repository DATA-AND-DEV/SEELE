// A janela do MOD de exemplo das ações declaradas (API 7).
//
// A leitura vai pelo `SeeleMods.request` de sempre; a operação protegida, pelo
// `SeeleMods.acao`, que só existe com a capacidade `acoes`. A recusa do
// servidor chega como a resposta: `{ ok: false, recusa: 'sem-permissao' }` ou
// `{ ok: false, recusa: 'acao-nao-declarada' }`.
const ID = 'prova/acoes';

async function fecharAMesa() {
  if (typeof SeeleMods.acao !== 'function') return { ok: false, erro: 'sem-a-capacidade-acoes' };
  return SeeleMods.acao(ID, 0, 'fechar-mesa', { op: 'fechar' });
}

async function lerAMesa() {
  return SeeleMods.request(ID, 0, { op: 'ler' });
}

globalThis.fecharAMesa = fecharAMesa;
globalThis.lerAMesa = lerAMesa;
