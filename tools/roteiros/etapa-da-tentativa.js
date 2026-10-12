// A linha de etapa é da tentativa mais nova (item 52 da pendência 58).
//
// Duas tentativas de entrada podem andar juntas: a nova começa (o
// `++tentativas` de `conectar`) enquanto a velha ainda espera o `connect`
// dela. No Rust, a geração separa os eventos das duas só a partir do `connect`
// da nova; antes dele — o apelido, a pergunta do nome, o
// `conferir_esta_versao` — as etapas da velha ainda chegam à janela. E o
// `finally` da velha apagava a linha de qualquer jeito. O W2 da F5 leu isso no
// código, e a bancada não emitia etapas.
//
// Aqui o duble emite as etapas como o `connect` as manda, com a tentativa que
// a janela passou a ele (`tentativa`):
//
// - **A.** Uma etapa da velha que chega com a nova antes do `connect` dela não
//   aparece como se fosse da nova.
// - **B.** A velha, ao terminar, não apaga a etapa da nova.
// - O `connect` leva a tentativa de quem o chamou.

const linha = () => {
  const el = document.getElementById("boot-etapa");
  return el.hidden ? "" : el.textContent.trim();
};
const etapa = (stage, tentativa) => window.__SEELE_EMITIR({ ConnectStageChanged: { stage, tentativa } });
const connects = () => window.__SEELE_HISTORICO.filter(([cmd]) => cmd === "connect").map(([, args]) => args);

// A velha, presa no `connect`.
let recusarAVelha;
SEELE_RESPOSTAS.connect = new Promise((_, recusar) => (recusarAVelha = recusar));
const velha = conectar("127.0.0.1:8383", "rafa", null);
exigir(await ate(() => connects().length === 1, 1500), "a tentativa velha não chegou ao connect");
const daVelha = tentativas;
etapa("Parada", daVelha);
exigir(linha() === ETAPAS.Parada, `a etapa da velha, com ela de pé, não apareceu: «${linha()}»`);

// A nova começa, e para no `conferir_esta_versao`, antes do `connect` dela.
let soltarAConferencia;
SEELE_RESPOSTAS.conferir_esta_versao = new Promise((resolver) => (soltarAConferencia = resolver));
const nova = conectar("127.0.0.1:8383", "rafa", null);
exigir(await ate(() => tentativas === daVelha + 1, 1500), "a tentativa nova não começou");
const daNova = tentativas;
await espera(100);

// A. Uma etapa da velha, com a nova ainda antes do `connect`.
etapa("Tentando", daVelha);
const metadeA = linha();
relatar(`A: com a nova antes do connect, uma etapa da velha deixou na linha «${metadeA}»`);

// B. A nova chega ao `connect` e diz a etapa dela; a velha termina.
SEELE_RESPOSTAS.connect = new Promise(() => {});
soltarAConferencia(null);
exigir(await ate(() => connects().length === 2, 1500), "a tentativa nova não chegou ao connect");
etapa("CaminhoAberto", daNova);
exigir(linha() === ETAPAS.CaminhoAberto, `a etapa da nova não apareceu: «${linha()}»`);
recusarAVelha({ error: "NotConnected", trail: [], substituida: true });
await velha;
await espera(100);
const metadeB = linha();
relatar(`B: depois de a velha terminar, a linha diz «${metadeB}»`);

exigir(metadeA === "", `uma etapa da velha apareceu como se fosse da nova: «${metadeA}»`);
exigir(metadeB === ETAPAS.CaminhoAberto, `a velha, ao terminar, apagou a etapa da nova: «${metadeB}»`);
const [doConnectVelho, doConnectNovo] = connects();
exigir(doConnectVelho?.tentativa === daVelha && doConnectNovo?.tentativa === daNova,
  `o connect não levou a tentativa de quem o chamou: ${doConnectVelho?.tentativa} e ${doConnectNovo?.tentativa}, e não ${daVelha} e ${daNova}`);
void nova;
