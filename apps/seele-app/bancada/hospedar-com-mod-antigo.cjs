// **Quem hospeda com um MOD de API antiga sabe ao hospedar** (tarefa 13 do
// plano da F3, Step 5b; o corte da API 6). Node puro, com o `frases.js` de
// verdade num contexto de VM.
//
// Depois do corte, um servidor 0.16 com um MOD da API 5 ligado recusaria todo
// app 0.16, e quem hospeda só ganhava uma queixa no registro. O app confere
// antes de subir (`mods_de_api_antiga`, em `src/mods.rs`) e devolve
// `FalhaAoHospedar::ModsDeApiAntiga { mods: [(id, api | null)] }`. Esta bancada
// mede o que a pessoa lê:
//
// 1. **Com a API lida**: o nome do MOD, a API, e as três saídas do dono (08/10),
//    nesta ordem — atualizar o MOD para a API 6, hospedar pelo Servidor 0.15 no
//    launcher, ou desligar o MOD e hospedar sem ele, com os dados ficando.
// 2. **Sem a API** (o pacote não está nesta máquina): a frase diz isso, com as
//    saídas de baixar o pacote ou desligar o MOD, e **sem número nenhum** —
//    «API 0» seria «não sei» escrito como um fato.
//
// Rodar:  node apps/seele-app/bancada/hospedar-com-mod-antigo.cjs

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const contexto = vm.createContext({ console });
vm.runInContext(fs.readFileSync(path.join(__dirname, "../ui/frases.js"), "utf8"), contexto);
const frase = (mods) => contexto.fraseDeErro({ ModsDeApiAntiga: { mods } });

// 1. Com a API lida.
const comApi = frase([["seele/estilo", 4]]);
assert.match(comApi, /seele\/estilo/, `a frase não diz qual MOD: ${comApi}`);
assert.match(comApi, /API 4/, `a frase não diz a API do MOD: ${comApi}`);
const saidas = [/[Aa]tualize o MOD para a versão da API 6/, /Servidor 0\.15, no launcher/, /desligue o MOD e hospede sem ele/];
let antes = -1;
for (const saida of saidas) {
  const onde = comApi.search(saida);
  assert.ok(onde > antes, `a saída ${saida} falta, ou saiu fora da ordem do dono: ${comApi}`);
  antes = onde;
}
assert.match(comApi, /os dados dele ficam/, `a saída de desligar não diz que os dados ficam: ${comApi}`);

// 2. Sem a API: o pacote não está aqui, e nenhum número.
const semApi = frase([["x/sumido", null]]);
assert.match(semApi, /x\/sumido/, `a frase sem API não diz qual MOD: ${semApi}`);
assert.match(semApi, /o pacote deste MOD não está nesta máquina/, `a frase sem API não diz que o pacote falta: ${semApi}`);
assert.doesNotMatch(semApi, /API \d/, `a frase sem API escreveu um número de API: ${semApi}`);
assert.match(semApi, /baixe-o de novo/, `a frase sem API não oferece baixar o pacote: ${semApi}`);
assert.match(semApi, /desligue o MOD/, `a frase sem API não oferece desligar o MOD: ${semApi}`);

// E um zero, que é «não sei» e não um número: o Rust manda `None`, mas um zero
// que chegasse por outro caminho não pode virar «API 0» na tela.
const zero = frase([["x/zero", 0]]);
assert.doesNotMatch(zero, /API 0/, `um zero virou «API 0» na frase: ${zero}`);
assert.match(zero, /o pacote deste MOD não está nesta máquina/, `o zero não foi dito como «não sei»: ${zero}`);

// E os dois juntos: cada MOD com a sua linha.
const juntos = frase([["seele/estilo", 4], ["x/sumido", null]]);
assert.ok(juntos.includes("seele/estilo: feito para a API 4") && juntos.includes("x/sumido: o pacote"),
  `com dois MODs, um deles sumiu da frase: ${juntos}`);

// 3. **As recusas das saídas** (`FalhaAoResolverModAntigo`, em `src/mods.rs`):
// cada variante, montada como o Tauri a manda — a de unidade como texto, a com
// campo como objeto —, sai com frase, e não com o recuo «ALGO FALHOU» nem com o
// nome da variante. A `BancoNaoRespondeu(String)` divide o nome com a de
// unidade da `FalhaNoMod`, e o guarda de nomes em `frontend.rs` não a separa:
// é aqui que ela é conferida.
const recusas = {
  ServidorNoAr: [/PARE A HOSPEDAGEM/, /servidor no ar nesta janela/],
  ServidorDesconhecido: [/NÃO ESTÁ ENTRE OS GUARDADOS/, /Nada foi mudado/],
  NaoEDeApiAntiga: [/JÁ NÃO IMPEDE/, /hospedar de novo/],
};
for (const [nome, esperadas] of Object.entries(recusas)) {
  const dita = contexto.fraseDeErro(nome);
  assert.doesNotMatch(dita, /ALGO FALHOU/, `«${nome}» caiu no recuo: ${dita}`);
  for (const esperada of esperadas) assert.match(dita, esperada, `a frase de «${nome}» não diz ${esperada}: ${dita}`);
}
const outroMod = contexto.fraseDeErro({ PacoteNaoServe: "conteudo-de-outro-mod" });
assert.match(outroMod, /NÃO SERVE: o pacote baixado é de outro MOD/, `a recusa do pacote não diz por quê: ${outroMod}`);
assert.match(outroMod, /continua como estava/, `a recusa do pacote não diz que nada mudou: ${outroMod}`);
const apiVelha = contexto.fraseDeErro({ PacoteNaoServe: "api-too-old" });
assert.match(apiVelha, /roda no Servidor 0\.15/, `um motivo do núcleo não sai pela frase dele: ${apiVelha}`);
const banco = contexto.fraseDeErro({ BancoNaoRespondeu: "database is locked" });
assert.doesNotMatch(banco, /ALGO FALHOU|BancoNaoRespondeu/, `a falha do banco caiu no recuo: ${banco}`);
assert.match(banco, /database is locked/, `a falha do banco não diz o que o banco disse: ${banco}`);
assert.match(banco, /continua como estava/, `a falha do banco não diz que nada mudou: ${banco}`);

console.log("hospedar com MOD antigo: a frase diz o MOD, a API e as três saídas na ordem do dono; sem o pacote, diz que ele falta, sem número; e toda recusa das saídas tem frase.");
