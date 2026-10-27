// A voz pela fronteira, DENTRO do `.wasm` do `seele-web` (Tarefa 3.3, lote A).
//
//     node xtask/web/voz.mjs <seele_web.wasm>
//
// As duas sequências fixas que o desktop grava no `seele-core`
// (`voice::relogio_gravado` e `voice::fontes_gravadas`), agora pela fronteira
// que a página vai chamar: o navegador tem de mandar os mesmos pacotes e
// separar as mesmas fontes. O JavaScript daqui faz o que a página faz: move
// bytes e alças, e não lê SSRC nenhum.
import { readFileSync } from "node:fs";

const [caminhoDoWasm] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(caminhoDoWasm), {});
const e = instance.exports;
const memoria = () => new Uint8Array(e.memory.buffer);
const saida = () => memoria().slice(e.saida_ptr(), e.saida_ptr() + e.saida_len());
function comBytes(bytes, f) {
  const p = e.alocar(bytes.length);
  memoria().set(bytes, p);
  try {
    return f(p, bytes.length);
  } finally {
    e.liberar(p, bytes.length);
  }
}
const falhas = [];
const confere = (caso, condicao, detalhe) => condicao || falhas.push(`${caso}: ${detalhe}`);
const igual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ---------------------------------------------------------------- o envio
// A fronteira abre o relógio em zero, e o desktop grava a sequência a partir de
// (65534, 14 400 000). Os mesmos eventos a partir de zero têm de dar os pacotes
// do desktop menos esse começo.
{
  e.voz_envio(77);
  const opus = new Uint8Array(40).fill(0xab);
  const pacotes = [];
  const datagrama = (carimbo, corpo) => comBytes(corpo, (p, n) => e.voz_empacotar(carimbo, p, n));
  const abre = (quantos, vazios) => {
    e.voz_quadro_passou();
    for (let indice = 0; indice < quantos; indice++) {
      const carimbo = e.voz_carimbo_do_quadro(quantos, indice);
      const codigo = datagrama(carimbo, vazios.includes(indice) ? new Uint8Array(0) : opus);
      if (codigo === 0) {
        const d = new DataView(saida().buffer);
        pacotes.push([d.getUint16(5), d.getUint32(7)]);
      } else if (codigo !== 2) {
        falhas.push(`envio: o datagrama voltou ${codigo}: ${new TextDecoder().decode(saida())}`);
      }
    }
  };
  e.voz_quadro_passou(); // calado
  e.voz_quadro_passou(); // o atraso da supressão
  abre(3, []);
  abre(1, [0]);
  e.voz_quadro_passou(); // mudo
  abre(1, []);
  e.voz_reabrir(77);
  abre(2, [0]);
  // A sequência fixa do desktop, deslocada para começar de (0, 0): o desktop
  // começa em (65534, 14 400 000), então cada pacote dele menos esse começo.
  const doDesktop = [
    [65_535, 14_400_960],
    [0, 14_401_920],
    [1, 14_402_880],
    [2, 14_405_760],
    [5, 14_408_640],
  ].map(([seq, carimbo]) => [(seq - 65_534 + 65_536) % 65_536, carimbo - 14_400_000]);
  confere("envio", igual(pacotes, doDesktop), `os pacotes saíram ${JSON.stringify(pacotes)}, e não ${JSON.stringify(doDesktop)}`);
}

// ------------------------------------- a partida pela hora da página
// A voz nova parte da hora que a página manda (`timeOrigin + now()`), e não do
// zero: depois de 15 e de 60 min de voz (tempo simulado, sem esperar), a nova
// parte à frente do fim da velha pelos 22 s fora. Sem a hora (a página antiga),
// o zero de antes, que é o que os blocos acima usam.
{
  const opus = new Uint8Array(40).fill(0xab);
  const carimboDoPacote = () => {
    const carimbo = e.voz_carimbo_do_quadro(1, 0);
    const codigo = comBytes(opus, (p, n) => e.voz_empacotar(carimbo, p, n));
    confere("partida", codigo === 0, `o datagrama voltou ${codigo}`);
    return new DataView(saida().buffer).getUint32(7);
  };
  for (const minutos of [15, 60]) {
    const comeco = 1_792_910_000_000;
    e.voz_envio(77, comeco);
    for (let q = 0; q < minutos * 60 * 50; q++) e.voz_quadro_passou();
    const fimDaVelha = carimboDoPacote();
    e.voz_envio(77, comeco + minutos * 60_000 + 22_000);
    e.voz_quadro_passou();
    const adiante = (carimboDoPacote() - fimDaVelha) | 0;
    confere("partida", adiante >= 22 * 48_000 && adiante <= 22 * 48_000 + 2 * 960, `depois de ${minutos} min, a voz nova partiu ${adiante} amostras do fim da velha`);
  }
}

// ------------------------------------------------------------- a recepção
{
  const EU = 5;
  const ANA = 7;
  const BIA = 9;
  e.voz_recepcao(EU);
  const de = (ssrc) => {
    const d = new DataView(new ArrayBuffer(13));
    d.setUint8(0, 1);
    d.setUint32(1, ssrc);
    d.setUint16(5, 1);
    d.setUint32(7, 960);
    d.setUint8(11, 0x2c);
    d.setUint8(12, 0xee);
    return new Uint8Array(d.buffer);
  };
  const alca = (ssrc, ms) => {
    const codigo = comBytes(de(ssrc), (p, n) => e.voz_chegou(p, n, ms));
    if (codigo === 2) return null;
    if (codigo !== 0) return `erro ${codigo}`;
    const s = saida();
    confere("recepção", igual([...s.slice(10)], [0x2c, 0xee]), "o Opus não atravessou inteiro");
    return new DataView(s.buffer).getUint32(0, true);
  };
  const esquecer = (ms) => {
    const quantas = e.voz_esquecer(ms);
    const d = new DataView(saida().buffer);
    return Array.from({ length: quantas }, (_, i) => d.getUint32(i * 4, true));
  };
  confere("recepção", alca(ANA, 0) === 1, "a Ana não ganhou a alça 1");
  confere("recepção", alca(BIA, 10) === 2, "a Bia não ganhou a alça 2");
  confere("recepção", alca(EU, 20) === null, "a própria voz entrou");
  confere("recepção", alca(ANA, 1_000) === 1, "a Ana mudou de alça");
  confere("recepção", igual(esquecer(30_005), []), "alguém foi esquecido cedo");
  confere("recepção", igual(esquecer(30_020), [2]), "a Bia não foi esquecida aos 30 s");
  confere("recepção", alca(BIA, 30_030) === 3, "a Bia voltou sem alça nova");
  confere("recepção", igual(esquecer(31_005), [1]), "a Ana não foi esquecida aos 30 s");
  const torto = comBytes(new Uint8Array(5), (p, n) => e.voz_chegou(p, n, 0));
  confere("recepção", torto === 1, `um datagrama torto voltou ${torto}`);
}

// ------------------------------------------------- o portão (lote C)
// O que a página chama por quadro de 20 ms: o portão, o relógio e o carimbo de
// cada quadro solto, numa chamada. Os mesmos casos dos testes nativos do
// `seele-web` (`voz::testes`), agora pela fronteira.
const QUADRO = 960;
const bytesDe = (f32) => new Uint8Array(f32.buffer, f32.byteOffset, f32.byteLength);
const CALADO = new Float32Array(QUADRO);
const FALA = new Float32Array(QUADRO).fill(0.3);
function capturou(pcm, mudo = false) {
  // `>>> 0`: o `.wasm` devolve i32, e o `u32::MAX` do erro chegaria como -1.
  const quantos = comBytes(bytesDe(pcm), (p, n) => e.voz_capturou(p, n, mudo ? 1 : 0)) >>> 0;
  if (quantos === 0xffff_ffff) return `erro: ${new TextDecoder().decode(saida())}`;
  const d = new DataView(saida().buffer);
  const passo = 4 + 4 * QUADRO;
  return Array.from({ length: quantos }, (_, i) => d.getUint32(i * passo, true));
}
function contasDoPortao() {
  confere("portão", e.voz_portao_contas() === 0, "as contas do portão falharam");
  return JSON.parse(new TextDecoder().decode(saida()));
}
{
  e.voz_envio(77);
  e.voz_modo(1);
  confere("portão", igual(capturou(CALADO), []), "o portão abriu no silêncio");
  confere("portão", igual(capturou(CALADO), []), "o portão abriu no silêncio");
  confere("portão", igual(capturou(FALA), [960, 1_920, 2_880]), "a abertura não levou os retidos com o carimbo de cada um");
  confere("portão", igual(capturou(FALA), [3_840]), "aberto, não saiu um quadro por quadro");

  e.voz_envio(77);
  e.voz_modo(2);
  confere("portão", igual(capturou(FALA, true), []), "no mudo saiu voz");
  confere("portão", contasDoPortao().nivel_dbfs > -20, "no mudo o medidor parou");
  confere("portão", igual(capturou(FALA), [1_920]), "no mudo o relógio parou");

  e.voz_envio(77);
  e.voz_regua(-50_000, 1);
  e.voz_modo(0);
  e.voz_tecla(1);
  e.voz_reabrir(78);
  confere("portão", capturou(CALADO).length === 1, "a tecla segurada não atravessou o reabrir");
  confere("portão", Math.abs(contasDoPortao().abertura_dbfs + 50) < 0.01, "a régua escolhida não atravessou o reabrir");

  const torto = comBytes(new Uint8Array(3), (p, n) => e.voz_capturou(p, n, 0)) >>> 0;
  confere("portão", torto === 0xffff_ffff, `um quadro que não é de f32 voltou ${torto}`);
}

// ------------------------------------- o teste do microfone (lote C)
// O portão da voz na ativação por voz, à parte da voz da sala: o silêncio não
// abre, a fala abre com os retidos, e o estado tem o corte da régua.
{
  e.voz_teste_abrir(0, 1);
  const quadro = (pcm) => comBytes(bytesDe(pcm), (p, n) => e.voz_teste_quadro(p, n)) >>> 0;
  confere("teste", quadro(CALADO) === 0 && quadro(CALADO) === 0, "o silêncio abriu o portão do teste");
  confere("teste", quadro(FALA) === 3, "a abertura do teste não levou os retidos");
  confere("teste", e.voz_teste_estado() === 0, "o estado do teste falhou");
  const estado = JSON.parse(new TextDecoder().decode(saida()));
  confere("teste", estado.aberto && estado.aberturas === 1 && estado.quadros_retidos === 2, `o estado não contou a abertura: ${JSON.stringify(estado)}`);
  confere("teste", Math.abs(estado.corte_dbfs + 60) < 0.01, `o corte não é o da régua com supressão: ${JSON.stringify(estado)}`);
  e.voz_teste_fechar();
  confere("teste", e.voz_teste_estado() === 1, "fechado, o teste continua contando");
}

// ---------------------------------------------- a reprodução (lote C)
// O que o worklet chama: um quadro decodificado por chegada e a mistura a cada
// 20 ms do relógio de áudio. O quadro 4 se perde, e a saída tem de descer a
// zero com fade e voltar com fade, sem degrau.
{
  const tocar = (perdidos, isolamento) => {
    e.voz_tocar();
    const saidas = [];
    for (let k = 0; k < 16; k++) {
      if (k < 10 && !perdidos.includes(k)) {
        const pcm = new Float32Array(QUADRO).fill(0.01 * (k + 1));
        const codigo = comBytes(bytesDe(pcm), (p, n) => e.voz_tocar_chegou(1, k, k * 960, k * 20, p, n));
        confere("reprodução", codigo === 0, `a chegada do quadro ${k} voltou ${codigo}`);
      }
      e.voz_tocar_isolado(isolamento.includes(k) ? 1 : 0);
      confere("reprodução", e.voz_tocar_quadro() === 0, "a mistura falhou");
      saidas.push(new Float32Array(saida().buffer));
    }
    return saidas;
  };
  const meios = (saidas) => saidas.map((q) => Math.round(q[QUADRO / 2] * 100));
  const livre = meios(tocar([], []));
  confere("reprodução", igual(livre.filter((c) => c !== 0), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), `os quadros não saíram em ordem: ${livre}`);

  const comPerda = tocar([4], []);
  const tocados = meios(comPerda);
  confere("reprodução", !tocados.includes(5), `o quadro perdido tocou: ${tocados}`);
  const continuo = comPerda.flatMap((q) => [...q]);
  const inicio = tocados.findIndex((c) => c !== 0) * QUADRO;
  const fim = (tocados.findLastIndex((c) => c !== 0) + 1) * QUADRO;
  let pior = 0;
  for (let i = inicio + 1; i < fim; i++) pior = Math.max(pior, Math.abs(continuo[i] - continuo[i - 1]));
  confere("reprodução", pior <= 0.0101, `um degrau de ${pior} no buraco do quadro perdido: ${tocados}`);
  confere("reprodução", e.voz_tocar_contas() === 0, "as contas da reprodução falharam");
  const contas = JSON.parse(new TextDecoder().decode(saida()));
  confere("reprodução", contas.fades >= 1, `o buraco não foi contado como fade: ${JSON.stringify(contas)}`);

  const isolado = meios(tocar([], [4, 5, 6, 7]));
  confere("reprodução", [4, 5, 6, 7].every((t) => isolado[t] === 0), `o isolamento deixou som sair: ${isolado}`);
  confere("reprodução", igual(isolado.slice(9), livre.slice(9)), `depois do isolamento a voz ficou atrasada: ${isolado} contra ${livre}`);

  const torto = comBytes(new Uint8Array(3), (p, n) => e.voz_tocar_chegou(1, 0, 0, 0, p, n));
  confere("reprodução", torto === 1, `um quadro que não é de f32 voltou ${torto}`);
}

if (falhas.length) {
  for (const f of falhas) console.error(`FALHOU — ${f}`);
  process.exit(1);
}
console.log("voz: o envio, o portão, o teste do microfone, a recepção e a reprodução passam pela fronteira, dentro do .wasm");
