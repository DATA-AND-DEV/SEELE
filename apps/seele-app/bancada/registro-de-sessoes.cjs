// O registro de sessões da página (`ui/sessao-web.js`, `criarRegistroDeSessoesWeb`),
// sem navegador: a função EXATA do arquivo num contexto do Node, com a
// abertura, a instância e a voz trocadas por promessas que o teste controla.
//
//   node apps/seele-app/bancada/registro-de-sessoes.cjs
//
// As intercalações que o Codex mediu em 24/10, e mais as do pedido:
// 1. **a queda com o soltar pendente, e uma entrada nova**: A cai (o
//    `aoAcabar`) enquanto `voz.soltar(A)` não terminou, e B entra. B NÃO
//    toma o foco sozinho (o foco vazio depois de uma queda é da pessoa), e o
//    áudio nunca tem dois donos;
// 2. **a troca com o alvo saindo**: `focarVoz(B)` espera o `soltar(A)`, e B
//    sai nesse meio. A troca é cancelada limpa: o foco fica vazio e o `tomar(B)`
//    não é chamado;
// 3. **o tomar que falha**: o foco fica vazio, e a falha é dita;
// 4. **o instanciar que falha**: a entrada rejeita, e nenhum registro fantasma
//    fica;
// 5. **a primeira entrada toma o foco**, e a segunda não;
// 6. **o portão**: em nenhum instante dois `podeUsarAVoz()` dizem sim.
//
// Sai com 1 no primeiro que falhar.

"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const fonte = fs.readFileSync(path.join(__dirname, "..", "ui", "sessao-web.js"), "utf8");
const contexto = vm.createContext({ setTimeout, clearTimeout, setInterval, clearInterval, console, performance, crypto: globalThis.crypto });
vm.runInContext(`${fonte}\nthis.criarRegistroDeSessoesWeb = criarRegistroDeSessoesWeb;`, contexto);
const { criarRegistroDeSessoesWeb } = contexto;

const falhas = [];
const confere = (caso, condicao, detalhe) => {
  if (!condicao) falhas.push(`${caso}: ${detalhe}`);
};
setTimeout(() => {
  console.error(`registro-de-sessoes: pendurou; o que já tinha falhado:\n  ${falhas.join("\n  ") || "(nada)"}`);
  process.exit(1);
}, 10_000).unref();

const tique = () => new Promise((r) => setTimeout(r, 5));

/** Uma promessa que o teste resolve ou rejeita quando quiser. */
function adiada() {
  let resolver;
  let rejeitar;
  const promessa = new Promise((a, b) => {
    resolver = a;
    rejeitar = b;
  });
  return { promessa, resolver, rejeitar };
}

/**
 * Uma montagem: a abertura falsa (cada sessão guarda o `aoAcabar` e o
 * `podeUsarAVoz` que o registro lhe deu), e a voz falsa, que anota os donos do
 * áudio e deixa o teste segurar cada `soltar` e cada `tomar`.
 */
function montar({ instanciarFalha = false } = {}) {
  const sessoes = new Map();
  const donos = new Set();
  const historico = [];
  let violacoes = 0;
  const segurarSoltar = new Map();
  const segurarTomar = new Map();
  const registro = criarRegistroDeSessoesWeb({
    instanciar: async () => {
      if (instanciarFalha) throw new Error("o .wasm não veio");
      return {};
    },
    abrir: async ({ alvo, aoAcabar, podeUsarAVoz }) => {
      const sessao = {
        alvo,
        aoAcabar,
        podeUsarAVoz,
        acabou: false,
        voltando: false,
        geracaoDoEnlace: 1,
        retrato: () => ({ sala: { server: alvo.endereco } }),
        async encerrar() {
          sessao.acabou = true;
        },
      };
      sessoes.set(alvo.endereco, sessao);
      return sessao;
    },
    voz: {
      async soltar(id) {
        historico.push(`soltar ${id}`);
        const segura = segurarSoltar.get(id);
        if (segura) await segura.promessa;
        donos.delete(id);
      },
      async tomar(id) {
        historico.push(`tomar ${id}`);
        if (donos.size > 0) violacoes += 1;
        const segura = segurarTomar.get(id);
        if (segura) await segura.promessa;
        donos.add(id);
      },
    },
  });
  const portaoAberto = () => [...sessoes.values()].filter((s) => s.podeUsarAVoz()).length;
  return { registro, sessoes, donos, historico, segurarSoltar, segurarTomar, violacoes: () => violacoes, portaoAberto };
}

const alvo = (nome) => ({ endereco: nome, impressao: "ab".repeat(32) });

(async () => {
  // ------------------------------------- 5. a primeira entrada toma o foco
  {
    const m = montar();
    const a = await m.registro.entrar({ alvo: alvo("A"), apelido: "ana" });
    const b = await m.registro.entrar({ alvo: alvo("B"), apelido: "ana" });
    confere("5", m.registro.foco() === a.id, `a primeira entrada não ficou com o foco: ${m.registro.foco()}`);
    confere("5", m.registro.frente() === b.id, "a segunda entrada não veio à frente");
    confere("5", !m.historico.includes(`tomar ${b.id}`), "a segunda entrada tomou o foco");
  }

  // ------------- 1. a queda com o soltar pendente, e uma entrada nova
  {
    const m = montar();
    const a = await m.registro.entrar({ alvo: alvo("A"), apelido: "ana" });
    const segura = adiada();
    m.segurarSoltar.set(a.id, segura);
    m.sessoes.get("A").aoAcabar("LinkLost");
    await tique();
    confere("1", m.portaoAberto() === 0, "A caiu e o portão continuou aberto");
    const b = await m.registro.entrar({ alvo: alvo("B"), apelido: "ana" });
    await tique();
    confere("1", m.registro.foco() !== b.id, `B tomou o foco sozinho depois da queda de A (foco ${m.registro.foco()})`);
    confere("1", m.donos.size <= 1, `o áudio teve ${m.donos.size} donos com o soltar(A) pendente: ${[...m.donos]}`);
    segura.resolver();
    await tique();
    confere("1", m.donos.size === 0 && m.registro.foco() === null, `depois do soltar(A): donos ${[...m.donos]}, foco ${m.registro.foco()}`);
    confere("1", m.violacoes() === 0, `um tomar começou com o áudio ainda dono de outro: ${m.historico.join(", ")}`);
  }

  // ------------------------------------- 2. a troca com o alvo saindo
  {
    const m = montar();
    const a = await m.registro.entrar({ alvo: alvo("A"), apelido: "ana" });
    const b = await m.registro.entrar({ alvo: alvo("B"), apelido: "ana" });
    const segura = adiada();
    m.segurarSoltar.set(a.id, segura);
    const troca = m.registro.focarVoz(b.id).then(
      (r) => ({ ok: r }),
      (e) => ({ erro: e }),
    );
    await tique();
    confere("2", m.portaoAberto() === 0, "durante a troca, alguém tinha o portão aberto");
    m.sessoes.get("B").aoAcabar("LinkLost");
    segura.resolver();
    const fim = await troca;
    await tique();
    confere("2", !m.historico.includes(`tomar ${b.id}`), `o tomar(B) foi chamado com B fora: ${m.historico.join(", ")}`);
    confere("2", m.registro.foco() === null && m.donos.size === 0, `depois da troca cancelada: foco ${m.registro.foco()}, donos ${[...m.donos]}`);
    confere("2", fim.erro !== undefined || fim.ok?.agora == null, `a troca cancelada resolveu como se tivesse trocado: ${JSON.stringify(fim)}`);
  }

  // ------------------------------------- 3. o tomar que falha
  {
    const m = montar();
    const a = await m.registro.entrar({ alvo: alvo("A"), apelido: "ana" });
    const b = await m.registro.entrar({ alvo: alvo("B"), apelido: "ana" });
    const falha = adiada();
    m.segurarTomar.set(b.id, falha);
    const troca = m.registro.focarVoz(b.id).then(
      () => "trocou",
      (e) => `falhou: ${e}`,
    );
    await tique();
    falha.rejeitar("o microfone recusou");
    const fim = await troca;
    confere("3", fim.startsWith("falhou"), `o tomar que falhou não foi dito: ${fim}`);
    confere("3", m.registro.foco() === null && m.portaoAberto() === 0, `com o tomar falho, o foco ficou ${m.registro.foco()}`);
    confere("3", m.historico.indexOf(`soltar ${a.id}`) < m.historico.indexOf(`tomar ${b.id}`), `a ordem foi ${m.historico.join(", ")}`);
  }

  // ------------------------------------- 4. o instanciar que falha
  {
    const m = montar({ instanciarFalha: true });
    const entrada = await m.registro.entrar({ alvo: alvo("A"), apelido: "ana" }).then(
      () => "entrou",
      () => "rejeitou",
    );
    confere("4", entrada === "rejeitou", `a entrada com o .wasm falho: ${entrada}`);
    confere("4", m.registro.sessoes().length === 0, `ficou registro fantasma: ${JSON.stringify(m.registro.sessoes())}`);
    // E uma entrada depois dela não herda nada.
    const m2 = montar();
    await m2.registro.entrar({ alvo: alvo("B"), apelido: "ana" });
    confere("4", m2.registro.sessoes().length === 1, "a entrada seguinte não ficou sozinha");
  }

  if (falhas.length) {
    console.error(`registro-de-sessoes: ${falhas.length} falha(s)\n  ${falhas.join("\n  ")}`);
    process.exit(1);
  }
  console.log("registro-de-sessoes: ok");
  process.exit(0);
})().catch((falha) => {
  console.error(falha);
  process.exit(1);
});
