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
  const datagrama = (carimbo, corpo) => comBytes(corpo, (p, n) => e.voz_datagrama(carimbo, p, n));
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

if (falhas.length) {
  for (const f of falhas) console.error(`FALHOU — ${f}`);
  process.exit(1);
}
console.log("voz: o envio e a recepção do desktop passam pela fronteira, dentro do .wasm");
