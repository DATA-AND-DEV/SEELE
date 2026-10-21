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
// **E aponta o Servidor 0.15 no launcher** (`compatibilidade.porque`; a
// auditoria do `v6.json`, tabela 1): dizer onde ele roda sem dizer onde
// abri-lo deixa quem lê sem o caminho.
assert.match(apiVelha, /use o Servidor 0\.15 pelo launcher/, `a recusa da API velha não aponta o launcher: ${apiVelha}`);
const banco = contexto.fraseDeErro({ BancoNaoRespondeu: "database is locked" });
assert.doesNotMatch(banco, /ALGO FALHOU|BancoNaoRespondeu/, `a falha do banco caiu no recuo: ${banco}`);
assert.match(banco, /database is locked/, `a falha do banco não diz o que o banco disse: ${banco}`);
assert.match(banco, /continua como estava/, `a falha do banco não diz que nada mudou: ${banco}`);

// 4. **A linha do pacote no cache diz o mesmo** (`linhaDePacoteNoCache`, em
//    `camada-mods.js`; a frase que o rascunho §2 cita): um pacote da API 4
//    recusado diz que roda no Servidor 0.15, e que ele se abre pelo launcher.
{
  const camada = fs.readFileSync(path.join(__dirname, "../ui/camada-mods.js"), "utf8");
  const i = camada.indexOf("function linhaDePacoteNoCache(pacote) {");
  const j = camada.indexOf("\n}\n", i);
  assert.ok(i >= 0 && j > i, "`linhaDePacoteNoCache` mudou de forma, e o recorte não a achou");
  const no = (tag, classe = "", texto = "") => ({
    tag, className: classe, textContent: String(texto ?? ""), filhos: [], dataset: {}, hidden: false,
    append(...outros) { this.filhos.push(...outros); },
    addEventListener() {}, setAttribute() {},
  });
  const linha = vm.createContext({
    elemento: no, emBytes: (n) => `${n} B`,
    API_DESTE_APLICATIVO: 6, APIS_QUE_ESTE_APLICATIVO_ACEITA: [6],
  });
  vm.runInContext(camada.slice(i, j + 3), linha);
  const textos = (n) => [n.textContent, ...n.filhos.flatMap(textos)].filter(Boolean);
  const dita = textos(linha.linhaDePacoteNoCache({ id: "seele/estilo", version: "3.1.0", bytes: 10, hash: "abc", recusado: true, api: 4 })).join(" | ");
  assert.match(dita, /feito para a API 4, que roda no Servidor 0\.15/, `a linha do pacote da API 4 não diz onde ele roda: ${dita}`);
  assert.match(dita, /use o Servidor 0\.15 pelo launcher/, `a linha do pacote da API 4 não aponta o launcher: ${dita}`);
}

// 5. **As recusas de instalar um MOD e de apagar um pacote do cache**
//    (pendência 58). Moram aqui porque a pergunta é a mesma das saídas acima:
//    o produto sabe por que recusou, e a frase tem de dizer isso a quem lê, e
//    não um código que ninguém sabe o que fazer com ele.
{
  // 5a. **O `mod.json` recusado diz o motivo pela frase de `RECUSAS_DE_PACOTE`**
  //     (item 8), a mesma da gestão e do pacote que não serve. Depois do corte
  //     da API 6, um MOD da API 5 instalado de uma pasta cai aqui, e a frase
  //     dizia «DESTA PASTA: api-too-old». Os códigos vêm do próprio dicionário,
  //     que o `todo_pacote_recusado_e_dito_com_uma_frase` (`frontend.rs`) cobra
  //     contra o `refusal_name` do núcleo.
  const recusasDePacote = vm.runInContext("RECUSAS_DE_PACOTE", contexto);
  const codigos = Object.keys(recusasDePacote);
  assert.ok(codigos.length >= 9, `\`RECUSAS_DE_PACOTE\` encolheu, e esta conferência compara com quase nada: ${codigos}`);
  // Todos os códigos antes de reprovar: saber que um só cai cru é outra
  // informação que saber que todos caem.
  const crus = codigos.filter((codigo) => {
    const dita = contexto.fraseDeErro({ ManifestoRecusado: codigo });
    return !dita.includes(recusasDePacote[codigo]) || dita.includes(`: ${codigo}.`);
  });
  assert.deepEqual(crus, [],
    `o «mod.json» recusado por estes códigos não sai pela frase de \`RECUSAS_DE_PACOTE\`, e a tela diz o código cru: ${crus.join(", ")}\n` +
    `por exemplo: ${contexto.fraseDeErro({ ManifestoRecusado: crus[0] ?? codigos[0] })}`);
  for (const codigo of codigos) {
    const dita = contexto.fraseDeErro({ ManifestoRecusado: codigo });
    assert.match(dita, /Nada foi instalado/, `a recusa do «mod.json» deixou de dizer que nada foi instalado: ${dita}`);
  }
  // E um código que o dicionário ainda não tem continua aparecendo cru, como
  // no resto das frases: feio, e honesto.
  const novo = contexto.fraseDeErro({ ManifestoRecusado: "um-codigo-novo" });
  assert.match(novo, /DESTA PASTA: um-codigo-novo\./, `um código sem frase sumiu da recusa: ${novo}`);

  // 5b. **A recusa local de um MOD do catálogo diz a recusa de instalar**
  //     (item 9; `FalhaNoCatalogo::NaoInstalou`, em `src/catalogo.rs`), e não
  //     culpa o catálogo nem a rede. O catálogo entregou os bytes certos: um
  //     disco cheio dizia «O CATÁLOGO VEIO QUEBRADO… Avise quem publica».
  //     Montadas como o serde as manda: a de instalar com campo, como objeto
  //     dentro do objeto.
  for (const recusa of [
    { NaoCopiei: "No space left on device (os error 28)" },
    { ManifestoRecusado: "api-too-new" },
    { TemAtalho: "/tmp/x/atalho" },
  ]) {
    const dita = contexto.fraseDeErro({ NaoInstalou: recusa });
    assert.doesNotMatch(dita, /CATÁLOGO|Avise quem publica|sua rede|ALGO FALHOU|NaoInstalou/,
      `a recusa local de um MOD do catálogo culpa o catálogo ou a rede, ou caiu no recuo: ${dita}`);
    assert.equal(dita, contexto.fraseDeErro(recusa),
      `a recusa local de um MOD do catálogo não diz a frase de instalar: ${dita}`);
  }
  const semManifesto = contexto.fraseDeErro({ NaoInstalou: "SemManifesto" });
  assert.equal(semManifesto, contexto.fraseDeErro("SemManifesto"),
    `a recusa de unidade dentro de \`NaoInstalou\` não diz a frase dela: ${semManifesto}`);
}

console.log("hospedar com MOD antigo: a frase diz o MOD, a API e as três saídas na ordem do dono; sem o pacote, diz que ele falta, sem número; e toda recusa das saídas tem frase.");
console.log("recusas de instalar: o «mod.json» recusado diz o motivo pela frase do código, e não o código cru; e a recusa local de um MOD do catálogo diz a de instalar, sem culpar o catálogo nem a rede.");
