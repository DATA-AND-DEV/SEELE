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
// **Verdadeira para toda API mais velha que a deste app** (rodada 1 do lote
// 58-1, M4). A frase dizia que o MOD «é de uma API que roda no Servidor 0.15»,
// e o `api-too-old` vale também para as APIs 0 a 2, que nenhum servidor roda.
// Ela diz quais o Servidor 0.15 roda, conferidas contra `APIS_POR_LINHA`
// (`crates/seele-proto/src/mods.rs`), a tabela de onde o app tira as dele: o
// `frases.js` é um dicionário estático, sem ponte para lê-la.
{
  const proto = fs.readFileSync(path.join(__dirname, "../../../crates/seele-proto/src/mods.rs"), "utf8");
  const tabela = /pub const APIS_POR_LINHA:[^=]*=\s*\[(.*?)\];/s.exec(proto);
  assert.ok(tabela, "`APIS_POR_LINHA` sumiu de crates/seele-proto/src/mods.rs, ou mudou de forma, e a frase do `api-too-old` não tem com o que se conferir");
  const porLinha = Object.fromEntries([...tabela[1].matchAll(/\("([^"]+)",\s*&\[([^\]]*)\]\)/g)]
    .map(([, linha, apis]) => [linha, apis.split(",").map((n) => Number(n.trim())).sort((a, b) => a - b)]));
  const legada = porLinha["0.15"];
  const desta = porLinha["1"];
  assert.ok(legada?.length > 0 && desta?.length === 1, `a leitura de \`APIS_POR_LINHA\` ficou cega: ${JSON.stringify(porLinha)}`);
  const [menor, maior] = [legada[0], legada.at(-1)];
  assert.equal(legada.length, maior - menor + 1,
    `as APIs do Servidor 0.15 deixaram de ser seguidas (${legada}), e «as APIs ${menor} a ${maior}» não as diz`);
  assert.doesNotMatch(apiVelha, /é de uma API que roda no Servidor 0\.15/,
    `a frase do \`api-too-old\` diz que o MOD roda no Servidor 0.15, e uma API ${menor - 1} não roda: ${apiVelha}`);
  assert.ok(apiVelha.includes(`que roda as APIs ${menor} a ${maior}`),
    `a frase do \`api-too-old\` não diz que o Servidor 0.15 roda as APIs ${menor} a ${maior}: ${apiVelha}`);
  assert.ok(apiVelha.includes(`mais velha que a ${desta[0]}`) && apiVelha.includes(`a versão dele para a API ${desta[0]}`),
    `a frase do \`api-too-old\` não diz a API deste app, ${desta[0]}: ${apiVelha}`);
}
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
    if (recusa.ManifestoRecusado) {
      // **Um MOD do catálogo não vem de pasta nenhuma** (rodada 1, I1): a de
      // passagem é do app e já foi apagada, e «DESTA PASTA» apontava para lugar
      // nenhum. O motivo é o mesmo do dicionário, e nada foi instalado.
      assert.doesNotMatch(dita, /PASTA/, `a recusa de um MOD do catálogo fala de uma pasta que ninguém escolheu: ${dita}`);
      assert.match(dita, /DESTE MOD/, `a recusa do «mod.json» de um MOD do catálogo não diz de quem é: ${dita}`);
      assert.ok(dita.includes(recusasDePacote[recusa.ManifestoRecusado]),
        `a recusa do «mod.json» de um MOD do catálogo não sai pela frase de \`RECUSAS_DE_PACOTE\`: ${dita}`);
      assert.match(dita, /Nada foi instalado/, `a recusa do «mod.json» de um MOD do catálogo não diz que nada foi instalado: ${dita}`);
    } else {
      assert.equal(dita, contexto.fraseDeErro(recusa),
        `a recusa local de um MOD do catálogo não diz a frase de instalar: ${dita}`);
    }
  }
  const semManifesto = contexto.fraseDeErro({ NaoInstalou: "SemManifesto" });
  assert.equal(semManifesto, contexto.fraseDeErro("SemManifesto"),
    `a recusa de unidade dentro de \`NaoInstalou\` não diz a frase dela: ${semManifesto}`);

  // 5c. **Apagar um pacote do cache** (item 10): os três motivos de
  //     `FalhaAoApagarPacote` (`src/mods.rs`), que o `apagar_pacote_do_cache`
  //     manda como `Recusado { motivo }`, saíam crus — «nao-apaguei» na tela, e
  //     nada sobre o que fazer.
  const doCache = {
    "nao-esta-aqui": /já não estava no cache/,
    "nao-apaguei": /feche o que usa o MOD e tente de novo/i,
    "hash-torto": /nada foi apagado/,
  };
  const crusDoCache = Object.keys(doCache).filter((motivo) => {
    const dita = contexto.fraseDeErro({ Recusado: { motivo } });
    return dita === motivo || dita.includes(motivo);
  });
  assert.deepEqual(crusDoCache, [], `estes motivos de apagar do cache saem crus na tela: ${crusDoCache.join(", ")}`);
  for (const [motivo, esperada] of Object.entries(doCache)) {
    const dita = contexto.fraseDeErro({ Recusado: { motivo } });
    assert.match(dita, esperada, `a frase de «${motivo}» não diz ${esperada}: ${dita}`);
  }
}

// 5d. **A busca automática do que um servidor já aceito exige** (rodada 1 do
//     lote 58-1, M2; `buscarOQueFaltaSeJaFoiAceito`, em `base.js`, recortada e
//     rodada aqui com o `frases.js` de verdade). A falha chega do Tauri como o
//     objeto do serde, e `String(objeto)` punha «[object Object]» na linha do
//     MOD, depois de «reconecte para tentar de novo —». A recusa local do item
//     9 nunca chegava a ser dita nesta rota.
(async () => {
  const base = fs.readFileSync(path.join(__dirname, "../ui/base.js"), "utf8");
  const i = base.indexOf("async function buscarOQueFaltaSeJaFoiAceito(catalogo, ausentes) {");
  const j = base.indexOf("\n}\n", i);
  assert.ok(i >= 0 && j > i, "`buscarOQueFaltaSeJaFoiAceito` mudou de forma, e o recorte não a achou");
  const falha = { NaoInstalou: { NaoCopiei: "No space left on device (os error 28)" } };
  const anotados = [];
  const busca = vm.createContext({
    console,
    alvoDaSessao: () => "casa.exemplo:4000",
    invoke: async (comando) => {
      if (comando === "aceite_de_mods") return "conjunto-aceito";
      if (comando === "instalar_mod_do_catalogo") throw falha;
      throw new Error(`comando inesperado: ${comando}`);
    },
    anotarEstadoDoMod: (id, fase, detalhe = "") => anotados.push({ id, fase, detalhe }),
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, "../ui/frases.js"), "utf8"), busca);
  vm.runInContext(`let conjuntoJaBuscado = "";\n${base.slice(i, j + 3)}`, busca);
  await busca.buscarOQueFaltaSeJaFoiAceito(
    { conjunto: "conjunto-aceito" },
    [{ id: "alguem/rpg", version: "1.0.0" }],
  );
  const naoObtive = anotados.find((a) => a.fase === "nao-obtive");
  assert.ok(naoObtive, `a busca que falhou não anotou o MOD como não obtido: ${JSON.stringify(anotados)}`);
  assert.doesNotMatch(naoObtive.detalhe, /\[object Object\]/,
    `a linha do MOD que não veio diz «[object Object]» no lugar do motivo: ${naoObtive.detalhe}`);
  assert.equal(naoObtive.detalhe, busca.fraseDeErro(falha),
    `a linha do MOD que não veio não diz a frase da recusa: ${naoObtive.detalhe}`);
  console.log("busca automática: o MOD que não veio diz a frase da recusa, e não «[object Object]».");
})().catch((erro) => {
  console.error(erro);
  process.exitCode = 1;
});

console.log("hospedar com MOD antigo: a frase diz o MOD, a API e as três saídas na ordem do dono; sem o pacote, diz que ele falta, sem número; e toda recusa das saídas tem frase.");
console.log("recusas de instalar: o «mod.json» recusado diz o motivo pela frase do código, e não o código cru; e a recusa local de um MOD do catálogo diz a de instalar, sem culpar o catálogo nem a rede; e apagar do cache tem frase para os três motivos.");
