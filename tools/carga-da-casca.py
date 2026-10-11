#!/usr/bin/env python3
"""Carrega a casca num navegador de verdade e diz o que estourou.

**Por que isto existe.** Os 156 guardas de `apps/seele-app/tests/frontend.rs`
leem a casca como *texto*: eles conferem que um id existe, que uma frase está
escrita, que uma classe não é definida duas vezes. Nenhum deles **executa** os
catorze scripts — e três defeitos passaram por todos eles ao mesmo tempo, em
versões que chegaram ao campo:

- um `let` com o nome de uma `function` de outro arquivo, que é `SyntaxError`
  no arquivo inteiro (os scripts dividem um escopo global só);
- uma chamada solta no topo de um script para uma função que não existe mais,
  que mata tudo o que era registrado depois dela — inclusive o `click` do
  `CONECTAR`;
- um valor calculado e nunca lido, que fazia hospedar continuar pedindo para
  conferir a própria chave.

Os dois primeiros ganharam guarda estático depois. O terceiro não tem como ter:
`const direto = …` seguido de um `if` que testa outra coisa é código válido e
plausível. Só carregar e apertar acha.

**O duble.** `window.__TAURI__` não existe fora do app, e sem ele `base.js`
morre na primeira linha. O duble aqui responde o suficiente para a casca chegar
à sessão; ele **não** finge um `Snapshot` fiel, e por isso o roteiro anula
`desenhar` e `atualizar`, que pintam a partir dele. O que este aparelho mede é
qual tela fica na frente e o que estoura no caminho — não o desenho, que é o que
uma foto responde melhor.

**A cópia.** A casca é copiada para uma pasta temporária antes de ser mexida.
Escrever a página de teste dentro de `ui/` já reprovou um guarda que lê tudo o
que mora lá — e um arquivo de teste esquecido naquela pasta é um arquivo que vai
para o instalador.

Uso:
    python3 tools/carga-da-casca.py            # só a carga
    python3 tools/carga-da-casca.py --roteiro tools/roteiros/hospedar.js
    python3 tools/carga-da-casca.py --roteiro tools/roteiros/a.js --roteiro tools/roteiros/b.js
    python3 tools/carga-da-casca.py --autoteste   # os consertos da carga, sem navegador

Com mais de um `--roteiro`, cada um roda com a sua carga, todos rodam mesmo
depois de um vermelho, e o fim diz quais reprovaram e com que frase. É assim
que o job `bancadas` do `ci.yml` e a bateria do `empacotar/publicar.sh` os
rodam, cada um com a lista inteira da pasta (o teste
`todo_roteiro_da_pasta_roda_no_ci_e_no_publicar`, em
`xtask/src/check_runtime.rs`, reprova o roteiro que faltar numa delas).
"""

import argparse
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import traceback
import urllib.parse

RAIZ = pathlib.Path(__file__).resolve().parent.parent
CASCA = RAIZ / "apps" / "seele-app" / "ui"
PROTO_MODS = RAIZ / "crates" / "seele-proto" / "src" / "mods.rs"
PROTO_VERSAO = RAIZ / "crates" / "seele-proto" / "src" / "version.rs"
CAPACIDADE_DA_JANELA = RAIZ / "apps" / "seele-app" / "capabilities" / "janela.json"


def apis_aceitas() -> str:
    """`APIS_ACEITAS` lida do Rust, e não copiada para cá.

    Um número escrito à mão no duble envelhece na primeira subida de API, e o
    catálogo do roteiro passa a oferecer o que o build de verdade filtraria.
    Falha alto se a constante mudar de forma: um duble que adivinha não prova.
    """
    achado = re.search(r"pub const APIS_ACEITAS: &\[u32\] = &\[([0-9, ]+)\];", PROTO_MODS.read_text())
    if not achado:
        raise SystemExit(f"não achei `APIS_ACEITAS` em {PROTO_MODS}; o duble precisa dela.")
    return "[" + achado.group(1) + "]"


def permissoes_da_janela() -> str:
    """As permissões que a capacidade `janela` concede, lidas dela.

    O duble recusa o que a ACL de verdade recusaria (`destroy`, `close`). A
    confirmação ao fechar da tarefa 21 passou neste roteiro com a janela que
    não fechava no app: o duble aceitava tudo, e a capacidade não dava o
    `destroy` com que o `@tauri-apps/api` termina o fechar (medido em 10/10).
    """
    import json

    permissoes = json.loads(CAPACIDADE_DA_JANELA.read_text()).get("permissions")
    if not isinstance(permissoes, list):
        raise SystemExit(f"não achei a lista `permissions` em {CAPACIDADE_DA_JANELA}; o duble precisa dela.")
    return json.dumps([p for p in permissoes if isinstance(p, str)])


def versao_do_servidor() -> str:
    """`VERSAO_DO_SERVIDOR` lida do Rust, pela mesma razão de `apis_aceitas`.

    É a versão da linha desta janela, a que `linhas_para_hospedar` devolve
    primeiro. Falha alto se a constante mudar de forma.
    """
    achado = re.search(r'pub const VERSAO_DO_SERVIDOR: &str = "([0-9]+\.[0-9]+)";', PROTO_VERSAO.read_text())
    if not achado:
        raise SystemExit(f"não achei `VERSAO_DO_SERVIDOR` em {PROTO_VERSAO}; o duble precisa dela.")
    return achado.group(1)


def linha_desta_janela() -> str:
    """A linha desta janela, com a mesma regra de `Linha::do_servidor`.

    `seele-lancador/src/linha.rs`: o Servidor 0.15 é a linha `0.15`, e do 1.0 em
    diante a linha é só o número maior (`1`).
    """
    maior, menor = versao_do_servidor().split(".")
    return f"0.{menor}" if maior == "0" else maior


DUBLE = """<script>
// O duble do Tauri. Responde o bastante para a casca andar; ver o cabeçalho de
// `tools/carga-da-casca.py` sobre o que ele deliberadamente não finge.
const SEELE_RETRATO = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR4nGP4FCSPFTEMLQkA4oZYwU22lhgAAAAASUVORK5CYII=";
const SEELE_APIS_ACEITAS = __APIS_ACEITAS__;
const SEELE_PERMISSOES_DA_JANELA = __PERMISSOES_DA_JANELA__;
// O comando da janela que a ACL de verdade recusaria é recusado aqui também,
// com a frase do Tauri.
const seelePermitido = (comando) => {
  if (!SEELE_PERMISSOES_DA_JANELA.includes(`core:window:allow-${comando}`)) {
    throw `window.${comando} not allowed. Permissions associated with this command: core:window:allow-${comando}`;
  }
};
const SEELE_VERSAO_DO_SERVIDOR = "__VERSAO_DO_SERVIDOR__";
const SEELE_LINHA = "__LINHA__";
const SEELE_RESPOSTAS = {
  hospedar: { aqui: "127.0.0.1:8383", convite: "seele://127.0.0.1:8383?fp=abc",
              alcance: "SoRedeLocal", porta_recusada: null, encontro_recusado: null },
};
// A mesma massa que a comp desenha, para as duas telas poderem ser postas lado
// a lado. Nomes de campo do `Snapshot` de `seele-ffi`, e não inventados: um
// campo com nome errado desenha um buraco, e o buraco parece defeito da casca.
const P = (id, nome, opcoes = {}) => ({
  id, nickname: nome, speaking: false, muted: false, total_isolation: false,
  signal: 96, sync_band: "Verde", is_self: false, ...opcoes,
});
const SEELE_QUADRO = {
  // `link` é o `LinkState` de `seele-ffi`: "Online", ou { InternalBattery: {…} }
  // quando cai. Um valor que o produto nunca manda faz a casca achar que caiu
  // (e o celular confiou num "Up" que não existe; ver 0.16/celular).
  caminho: "UPNP", link: "Online", link_state: "Verified", server: "TÓQUIO-3",
  icon_revision: 0, me: 1, nickname: "rafa",
  voice_rooms: [
    { id: 1, name: "PONTE", limit: 8, password_required: false, occupied_by_us: true,
      channel: null, sync: null,
      people: [P(1, "rafa", { is_self: true }), P(2, "juli", { speaking: true, signal: 88 }),
               P(3, "vitor", { signal: 71, sync_band: "Laranja" })] },
    { id: 2, name: "OFICINA", limit: 6, password_required: false, occupied_by_us: false,
      channel: null, sync: null, people: [P(4, "dani", { signal: 54, sync_band: "Laranja" })] },
    { id: 3, name: "SILÊNCIO", limit: 4, password_required: false, occupied_by_us: false,
      channel: null, sync: null, people: [] },
  ],
  presentes: [P(1, "rafa", { is_self: true }), P(2, "juli"), P(3, "vitor"), P(4, "dani")],
  channels: [
    { id: 1, name: "geral", open: true }, { id: 2, name: "obras", open: false },
    { id: 3, name: "links", open: false }, { id: 4, name: "ruido", open: false },
  ],
  messages_revision: 1,
  telemetry: { rtt_ms: 23, jitter_ms: 4, loss_fraction: 0.002, bitrate_bps: 48000,
               signal: 92, sync_band: "Verde", input_level: 0.4, local_fault: false,
               frames_refused: 0 },
  notice: null, muted: false, total_isolation: false, speaking: false,
  person_icons_revision: 1,
  voice_mode: "PushToTalk", audio_available: true, capture: null, playback: null,
  // Dentro de sala, o áudio abriu: o Snapshot de verdade sempre traz o estado da
  // abertura (pendência 52), e sem ele o rodapé e o medidor caem no «fora de sala».
  abertura_do_audio: "aberto",
  may_manage_voice_rooms: true, may_kick: true, may_ban: true, may_remove_message: true,
  may_move_person: true, may_customise_server: true, may_delete_rooms: true,
  tela: null, ended: null,
};
const M = (id, autor, nome, quando, corpo, propria = false) => ({
  id, channel: 1, author: autor, author_nickname: nome, at_seconds: quando,
  body: corpo, own: propria, edited: false, attachment: null,
});
SEELE_RESPOSTAS.messages = [
  M(1, 2, "juli", 1756000442, "subi o seeled aqui de casa, a porta 8383 tá aberta no roteador"),
  M(2, 3, "vitor", 1756000480, "UDP? levei meia hora pra descobrir que a regra que eu tinha escrito era TCP"),
  M(3, 1, "rafa", 1756000511, "UDP. tá escrito no README, duas linhas depois do comando", true),
  M(4, 2, "juli", 1756000563, "o furo de NAT funcionou daqui pro celular do vitor, o encontro só apresentou os dois e esqueceu"),
  M(5, 4, "dani", 1756000649, "entrei na oficina, alguém vem?"),
];
SEELE_RESPOSTAS.snapshot = SEELE_QUADRO;
SEELE_RESPOSTAS.connect = { snapshot: SEELE_QUADRO, veredito: null };
window.__SEELE_CHAMADAS = [];
window.__SEELE_ARGS = {};
// A sequência inteira, e não só o último de cada comando: «apertou e soltou» é
// uma ordem, e o último sozinho não diz se o meio aconteceu.
window.__SEELE_HISTORICO = [];
window.__SEELE_VOLUMES = {};
window.__SEELE_EM_SESSAO = false;
// **Hospedar não é estar em sessão.** `connect` entra no servidor de outro e
// não hospeda nada, e o `estado_do_servidor` do produto só diz `hospedando:
// true` com uma `Hospedagem` aberta nesta janela. `parar_de_hospedar` a
// derruba, como no Rust; o `disconnect` não, desde a tarefa 19 da F7.
window.__SEELE_HOSPEDANDO = false;
// As mensagens de apps mais novos que o servidor hospedado pulou. Um roteiro
// troca os dois números para ver a linha de ESTE SERVIDOR com cada um.
window.__SEELE_APPS_MAIS_NOVOS = { mensagens: 0, sessoes: 0 };
// **Uma máquina que já escolheu o nome**, o mesmo do quadro. Desde a `acbc1c29`
// a primeira entrada de uma máquina sem nome abre «COMO OS OUTROS VÃO TE
// CHAMAR?» antes do `connect` e espera a resposta; com `""` aqui, todo roteiro
// que passa por `entrarNaSessao` parava nessa pergunta. Um roteiro sobre a
// máquina sem nome põe `""` antes de entrar.
window.__SEELE_APELIDO = "rafa";
window.__SEELE_SERVIDORES = [];
window.__SEELE_SEM_RESPOSTA = new Set();
window.__SEELE_OUVINTES = {};
window.__SEELE_EMITIR = (carga) => {
  for (const ouvinte of window.__SEELE_OUVINTES["seele://event"] ?? []) {
    ouvinte({ payload: carga });
  }
};
window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      window.__SEELE_CHAMADAS.push(cmd);
      // Os argumentos de cada comando, para um roteiro poder afirmar sobre o
      // que a casca **mandou** e não só sobre o que ela chamou. No duble e não
      // num embrulho de fora: os módulos capturam o `invoke` na carga, e trocá-lo
      // depois não alcança ninguém.
      window.__SEELE_ARGS[cmd] = args;
      window.__SEELE_HISTORICO.push([cmd, args]);
      // O MUDO e o SURDO mudam o quadro, como no produto: uma tela que espera o
      // próximo quadro dizer «mudo» antes de seguir só anda se ele disser.
      if (cmd === "set_muted") SEELE_QUADRO.muted = !!(args && args.on);
      if (cmd === "set_total_isolation") SEELE_QUADRO.total_isolation = !!(args && args.on);
      // O volume por pessoa fica guardado, e o `volumes_locais` o devolve, como
      // o Rust. A casca da API 6 relê os volumes e repinta os deslizantes: com o
      // duble respondendo vazio, a releitura devolvia todo volume a 100 (medido:
      // 16 em 40 trocas, na rodada da F3).
      if (cmd === "set_volume" && args) window.__SEELE_VOLUMES[args.nickname] = args.percent;
      if (cmd === "volumes_locais") return { ...window.__SEELE_VOLUMES };
      // **Sem sessão, `snapshot` recusa** — como o produto faz. Responder um
      // quadro na tela inicial faria o duble mentir sobre a única pergunta que
      // separa os dois modos do diálogo de perfil, e um aparelho que mente
      // sobre isso não prova nada sobre ele.
      if (cmd === "connect" || cmd === "hospedar") window.__SEELE_EM_SESSAO = true;
      if (cmd === "disconnect") window.__SEELE_EM_SESSAO = false;
      if (cmd === "hospedar") window.__SEELE_HOSPEDANDO = true;
      // Desde a tarefa 19 da F7, sair não derruba a hospedagem: quem derruba é
      // o `parar_de_hospedar`, como no Rust.
      if (cmd === "parar_de_hospedar") window.__SEELE_HOSPEDANDO = false;
      // O servidor guardado no ar, no formato do `MeuServidor` de `main.rs`.
      if (cmd === "meu_servidor") {
        return window.__SEELE_HOSPEDANDO ? { alvo: "127.0.0.1:8383", nome: "Casa" } : null;
      }
      if (cmd === "snapshot" && !window.__SEELE_EM_SESSAO) throw "NotConnected";
      // **Um comando pode recusar.** Sem isto o duble só sabia responder, e
      // metade da casca — toda tela que existe por causa de uma recusa — não
      // tinha como ser exercitada aqui. Um roteiro põe
      // `SEELE_RESPOSTAS.connect = { __recusa: {...} }` e o `invoke` estoura
      // com o objeto de dentro, que é a forma que o Tauri entrega.
      if (cmd in SEELE_RESPOSTAS) {
        const resposta = SEELE_RESPOSTAS[cmd];
        if (resposta && typeof resposta === "object" && "__recusa" in resposta) {
          throw resposta.__recusa;
        }
        return resposta;
      }
      if (/conhecid|fontes|lista|dispositiv|visitad|salas|canais|pessoas|^microfones$|^saidas$/i.test(cmd)) return [];
      // **Os três comandos do caminho de HOSPEDAR AQUI**, com o formato de
      // `servidores::Servidor` e `versoes::VersaoInstalada`. Sem eles o `null`
      // de baixo estourava em `guardados.length` e `guardado.id`, dentro de
      // `try`s que só fazem `console.warn` — e a casca ficava na entrada com a
      // carga limpa. Um roteiro pode pôr servidores em `__SEELE_SERVIDORES`.
      if (cmd === "servidores_guardados") return window.__SEELE_SERVIDORES;
      if (cmd === "versoes_instaladas") return [];
      // **A versão do servidor desta janela, e só ela**, com o formato de
      // `versoes::LinhaParaHospedar`. O produto devolve sempre ao menos esta,
      // primeiro e escolhida; as outras vêm de um catálogo guardado que o
      // duble não tem. Sem esta linha o `null` estourava em `linhas.length`
      // dentro de `desenharVersoesParaHospedar`, desde a `a2160d75`: a tela de
      // preparar já estava na frente, o `TypeError` caía num `console.warn` e
      // o bloco das versões nunca era desenhado. Nenhum roteiro reprovava por
      // isso, e é por isso que ninguém viu.
      if (cmd === "linhas_para_hospedar") {
        return [{ linha: SEELE_LINHA, servidor: SEELE_VERSAO_DO_SERVIDOR, apis: SEELE_APIS_ACEITAS,
                  app: null, esta: true, instalada: true, tem_pacote: true, perde_conversas: false }];
      }
      // Com `null`, `ultimaQueEsteBuildEntende` estourava em `.length` e o
      // catálogo inteiro virava «ALGO FALHOU».
      if (cmd === "apis_de_mod_aceitas") return SEELE_APIS_ACEITAS;
      if (cmd === "pacotes_no_cache") return [];
      if (cmd === "criar_servidor") {
        const nome = ((args && args.nome) || "").trim();
        const raiz = nome.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
          .slice(0, 32) || "servidor";
        let id = raiz;
        for (let n = 2; window.__SEELE_SERVIDORES.some((s) => s.id === id); n++) id = `${raiz}-${n}`;
        const novo = { id, nome, versao: "", caminho: `servidores/${id}/seele.db`, ultimo_uso: 0 };
        window.__SEELE_SERVIDORES.push(novo);
        return novo;
      }
      // Um retrato para uma pessoa só: o que se quer ver é a diferença entre um
      // avatar com imagem e um com iniciais, lado a lado no mesmo quadro.
      if (cmd === "imagem_da_pessoa") return args && args.person === 2 ? SEELE_RETRATO : null;
      // O retrato desta máquina, que é o que faz os dois diálogos de perfil
      // terem o mesmo conteúdo com sessão e sem.
      if (cmd === "meu_retrato") return SEELE_RETRATO;
      // Os bytes do retrato do servidor, que o roteiro pode trocar para provar
      // que a tela acompanha uma mudança.
      if (cmd === "icone_do_server") return window.__SEELE_ICONE_DO_SERVER ?? null;
      // O formato do `EstadoDoServidor` de `main.rs`. Quem não hospeda recebe
      // os zeros, como o `Default` de lá: não há servidor de que falar.
      if (cmd === "estado_do_servidor") {
        const hospedando = window.__SEELE_HOSPEDANDO;
        const { mensagens, sessoes } = window.__SEELE_APPS_MAIS_NOVOS;
        return { hospedando,
                 mensagens_desconhecidas: hospedando ? mensagens : 0,
                 sessoes_com_desconhecidas: hospedando ? sessoes : 0 };
      }
      // O apelido desta máquina, como as preferências o guardam. O roteiro
      // pode trocá-lo por `window.__SEELE_APELIDO`.
      if (cmd === "apelido_local") return window.__SEELE_APELIDO ?? "";
      if (cmd === "escolher_apelido_local") {
        window.__SEELE_APELIDO = args && args.apelido;
        return null;
      }
      // Os controles de voz no padrão do produto (`ControlesDaVoz`, seele-ffi):
      // a supressão ligada (SUPRESSAO_PADRAO), a abertura de −60 dBFS que vale
      // com ela, ninguém escolheu, e a faixa de FAIXA_DE_ABERTURA_DBFS. Com o
      // `null`, a seção ÁUDIO estourava ao abrir e desenhava a régua sem os
      // extremos.
      // A permissão do microfone: o roteiro pode trocá-la por
      // `window.__SEELE_PERMISSAO_DO_MICROFONE` (a ponte do navegador devolve
      // também a `NegadaPeloNavegador`).
      if (cmd === "permissao_de_microfone") return window.__SEELE_PERMISSAO_DO_MICROFONE ?? "Permitida";
      if (cmd === "controles_da_voz") {
        return { supressao: 1, abertura_dbfs: -60, abertura_escolhida: false,
                 abertura_minima_dbfs: -72, abertura_maxima_dbfs: -24 };
      }
      if (/apelido|nickname|preferenc|link|caminho/i.test(cmd)) return "";
      // **O que caiu aqui fica anotado.** Metade dos comandos responde `null`
      // de verdade; a outra metade devolve uma lista ou um objeto, e o `null`
      // vira um TypeError dentro de um `try` que só faz `console.warn`. O
      // aparelho imprime esta lista no fim para o próximo comando novo da
      // casca aparecer dito, e não como uma tela que não trocou.
      window.__SEELE_SEM_RESPOSTA.add(cmd);
      return null;
    },
    convertFileSrc: (p) => p,
  },
  // Os ouvintes ficam guardados: um roteiro pode emitir eventos do produto —
  // `ScreenOpened`, `ScreenFrame` — e testar a metade da casca que só existe
  // em resposta a eles.
  event: {
    listen: async (nome, ouvinte) => {
      (window.__SEELE_OUVINTES[nome] ||= []).push(ouvinte);
      return () => {};
    },
    emit: async () => {},
  },
  // O ouvinte do fechar fica guardado, e o `close` é contado: um roteiro pode
  // pedir o fechar da janela como o sistema pede, e ver se ela fechou.
  window: { getCurrentWindow: () => ({
    minimize: async () => {}, toggleMaximize: async () => {},
    // **O fechar como o Tauri 2 o faz.** Com um ouvinte de `onCloseRequested`
    // de pé, o `close()` vira um pedido de fechar, o ouvinte decide, e sem
    // `preventDefault` quem fecha é o `destroy()` do embrulho — que precisa da
    // permissão dele. Sem ouvinte, o `close()` fecha. `__SEELE_DESTRUIDA` diz
    // se a janela fechou de fato.
    close: async () => {
      seelePermitido("close");
      window.__SEELE_FECHAMENTOS = (window.__SEELE_FECHAMENTOS ?? 0) + 1;
      if (window.__SEELE_AO_FECHAR) await window.__SEELE_AO_FECHAR();
      else window.__SEELE_DESTRUIDA = true;
    },
    destroy: async () => { seelePermitido("destroy"); window.__SEELE_DESTRUIDA = true; },
    isFullscreen: async () => false, setFullscreen: async () => {},
    // `__SEELE_AO_FECHAR()` é o sistema pedindo o fechar: devolve o pedido,
    // com `segurado` dizendo se o ouvinte o vetou.
    onCloseRequested: async (ouvinte) => {
      window.__SEELE_AO_FECHAR = async () => {
        const pedido = {
          segurado: false,
          preventDefault() { this.segurado = true; },
          isPreventDefault() { return this.segurado; },
        };
        await ouvinte(pedido);
        if (!pedido.isPreventDefault()) await window.__TAURI__.window.getCurrentWindow().destroy();
        return pedido;
      };
      return () => {};
    },
    listen: async () => () => {},
  }) },
  webviewWindow: { getCurrentWebviewWindow: () => ({ listen: async () => () => {} }) },
  dialog: { open: async () => null, save: async () => null, message: async () => {} },
  fs: { readFile: async () => new Uint8Array() },
  updater: { check: async () => null },
};
</script>
"""

CABECA_DO_ROTEIRO = """<script>
window.__SEELE_RELATO = [];
// **Sempre texto.** O relato viaja em JSON (ver `PE_DO_ROTEIRO`), que guarda o
// tipo: um `relatar(n)` com um número chegava ao aparelho como número, e um
// `relatar(undefined)` como `null`, e os dois o derrubavam. Antes do JSON, o
// `join` convertia tudo em texto, e é isso que volta a valer.
const relatar = (linha) => window.__SEELE_RELATO.push(String(linha));
const espera = (ms) => new Promise((r) => setTimeout(r, ms));
const visivel = (id) => {
  const el = document.getElementById(id);
  return el ? (el.hidden ? "escondida" : "VISIVEL") : "AUSENTE";
};
const telas = (quando) =>
  `${quando}: boot=${visivel("tela-boot")} auth=${visivel("tela-auth")} sessao=${visivel("tela-sessao")}`;
// **Um roteiro pode reprovar.** Relatar só não basta: `sessao.js` relatou
// `sessao=escondida` por semanas com a carga limpa no fim, e quem lê a última
// linha lê «limpa». `exigir` escreve REPROVADO, para o roteiro ali e faz o
// aparelho sair com 1.
const exigir = (condicao, frase) => {
  if (condicao) return;
  relatar("REPROVADO: " + frase);
  throw Object.assign(new Error(frase), { reprovado: true });
};
const ate = async (condicao, ms) => {
  for (let t = 0; t < ms && !condicao(); t += 50) await espera(50);
  return condicao();
};
// HOSPEDAR AQUI até a sessão, pelo caminho que a pessoa faz hoje: o botão abre
// a tela de preparar (ou a lista, havendo servidor guardado), e é CONFIRMAR que
// hospeda. O diálogo da porta fecha porque cobre a tela. Quem chama recebe a
// sessão na frente **ou** a reprovação — nunca a entrada calada.
const entrarNaSessao = async () => {
  document.getElementById("botao-hospedar").click();
  if (await ate(() => visivel("tela-preparar") === "VISIVEL", 900)) {
    document.getElementById("preparar-confirmar").click();
  }
  await ate(() => visivel("tela-sessao") === "VISIVEL", 1200);
  await espera(200);
  const porta = document.getElementById("porta");
  if (porta && !porta.hidden) {
    document.getElementById("porta-entendi").click();
    await espera(200);
  }
  exigir(visivel("tela-sessao") === "VISIVEL",
    telas("a sessão não ficou visível depois de HOSPEDAR AQUI") +
      ` preparar=${visivel("tela-preparar")} servidores=${visivel("tela-servidores")}`);
};
setTimeout(async () => {
  try {
"""

PE_DO_ROTEIRO = """
  } catch (erro) {
    if (!(erro && erro.reprovado)) relatar("ROTEIRO ESTOUROU: " + erro);
  }
  // **Codificado, para atravessar o registro do navegador inteiro.** O Chromium
  // escreve a linha entre aspas duplas e não escapa as de dentro, e o aparelho
  // lia até a primeira: uma aspa num relato **antes** de um `exigir` que
  // reprovava escondia o REPROVADO, e a carga saía limpa (medido no lote RT).
  console.log("SEELE-RELATO " + encodeURIComponent(JSON.stringify(window.__SEELE_RELATO)));
  console.log("SEELE-SEM-RESPOSTA " + [...window.__SEELE_SEM_RESPOSTA].sort().join(" "));
}, 800);
</script>
"""


def navegador() -> str:
    """O primeiro `chrome-headless-shell` que este computador já tem."""
    for raiz in [
        pathlib.Path.home() / "Library" / "Caches" / "ms-playwright",
        pathlib.Path.home() / ".cache" / "ms-playwright",
        pathlib.Path.home() / ".cache" / "puppeteer",
    ]:
        if not raiz.exists():
            continue
        achados = sorted(raiz.rglob("chrome-headless-shell"))
        if achados:
            return str(achados[-1])
    print(
        "não achei um `chrome-headless-shell`. Playwright ou Puppeteer instalam um;\n"
        "`npx playwright install chromium` basta.",
        file=sys.stderr,
    )
    raise SystemExit(2)


def rodar_navegador(comando: list[str], prazo_s: float) -> tuple[str, bool]:
    """Roda o navegador e devolve o que ele escreveu e se o prazo venceu.

    **Num grupo de processos só dele, e o prazo mata o grupo inteiro.** Um
    roteiro que gira para sempre (`for (;;) {}`) prende o renderer, e o
    navegador nunca sai, nem com o orçamento de tempo virtual. Sem prazo aqui,
    quem parava a carga era o prazo de quem chamou, e o `kill -9` da etapa do
    `publicar.sh` atinge só o subshell: o Python da carga e o
    `chrome-headless-shell` ficavam órfãos, com o renderer a 100% de CPU, depois
    de 40 minutos de espera (M2 da revisão do lote RT, medido de novo aqui).

    O grupo, e não só o processo principal: o renderer, o da GPU e o
    utilitário são filhos dele. No macOS, matar só o principal bastou (medido:
    nenhum filho vivo 3 s depois); no Linux do CI não foi medido, e o grupo
    cobre os dois sem depender de como cada sistema trata os filhos.
    """
    processo = subprocess.Popen(
        comando,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    try:
        saida, erro = processo.communicate(timeout=prazo_s)
        return saida + erro, False
    except subprocess.TimeoutExpired:
        matar_o_grupo(processo)
        saida, erro = processo.communicate()
        return (saida or "") + (erro or ""), True
    except BaseException:
        # **Qualquer outra saída antecipada mata o grupo também.** O grupo
        # próprio põe o navegador fora do alcance do Ctrl+C do terminal, que vai
        # só ao grupo da frente: interromper à mão uma carga com um roteiro que
        # trava deixava 5 processos órfãos, o renderer a 100% (R2-M2 da revisão
        # do lote RT, medido de novo aqui). O `KeyboardInterrupt` chega aqui
        # sozinho; o SIGTERM, pelo tratador que `main` instala.
        matar_o_grupo(processo)
        raise


def matar_o_grupo(processo: subprocess.Popen) -> None:
    """Mata o grupo de processos do navegador, se ele ainda existir, e colhe o
    principal, que é filho da carga e ficaria zumbi até ela sair."""
    try:
        os.killpg(processo.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        processo.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass


def terminar_pelo_sinal(numero: int, _quadro) -> None:
    """O SIGTERM vira `SystemExit`, que passa pelo `except` de
    `rodar_navegador` e mata o navegador antes de a carga sair. Sem isto, o
    Python sai na hora e não limpa nada."""
    raise SystemExit(128 + numero)


def argumentos_da_carga() -> argparse.ArgumentParser:
    """Os argumentos da carga, num lugar só: `main` os lê, e o autoteste
    confere a leitura deles sem abrir navegador nenhum."""
    argumentos = argparse.ArgumentParser(description=__doc__)
    # Repetível. Antes, o segundo `--roteiro` tomava o lugar do primeiro em
    # silêncio, e um vermelho na frente da lista sumia sem ter rodado.
    argumentos.add_argument("--roteiro", type=pathlib.Path, action="append", default=None)
    argumentos.add_argument("--janela", default="1440,900")
    # Para comparar contra uma versão antiga: aponte para o `ui/` de um
    # `git worktree`, e o mesmo roteiro roda contra o passado.
    argumentos.add_argument("--casca", type=pathlib.Path, default=CASCA)
    # A foto é tirada sempre; isto só a guarda em vez de deixá-la sumir com a
    # pasta temporária. Ver o que a tela ficou é metade do diagnóstico.
    argumentos.add_argument("--foto", type=pathlib.Path, default=None)
    # Todo `console.*` da página, e não só os estouros: quando a casca falha em
    # silêncio, o `console.warn` é o que ela tinha para dizer.
    argumentos.add_argument("--tudo", action="store_true")
    # Quanto um roteiro pode levar, em segundos de verdade, antes de a carga
    # matar o navegador e reprová-lo. O orçamento de tempo virtual é de 5 s, e
    # cada roteiro de hoje leva menos de um segundo neste Mac; 60 é folga para
    # um runner lento, e não para um roteiro que gira para sempre (ver
    # `rodar_navegador`).
    argumentos.add_argument("--prazo-por-roteiro", type=float, default=60.0)
    # Sem navegador: confere os consertos da carga contra o código dela (ver
    # `autoteste`). O `cargo test -p xtask` o roda.
    argumentos.add_argument("--autoteste", action="store_true")
    return argumentos


def main() -> int:
    signal.signal(signal.SIGTERM, terminar_pelo_sinal)
    argumentos = argumentos_da_carga()
    opcoes = argumentos.parse_args()
    if opcoes.autoteste:
        return autoteste()
    roteiros = opcoes.roteiro or []
    if len(roteiros) > 1 and opcoes.foto:
        argumentos.error("--foto guarda a foto de um roteiro só; com vários, "
                         "cada um cobriria a do anterior.")
    if len(roteiros) > 1:
        return varios(opcoes, roteiros)
    return 1 if carregar_sem_cair(opcoes, roteiros[0] if roteiros else None) else 0


def carregar_sem_cair(opcoes: argparse.Namespace, roteiro: pathlib.Path | None) -> list[str]:
    """`carregar`, e um defeito do aparelho num roteiro vira motivo daquele
    roteiro, e não uma lista parada no meio.

    **A carga nunca pode cair.** Um traceback no meio de `varios` parava a
    lista ali: os roteiros seguintes não rodavam, e o resumo que diz quem
    reprovou não saía (M1 da revisão do lote RT). O traceback continua sendo
    escrito, porque é o diagnóstico. A falta do navegador não passa por aqui:
    ela é `SystemExit`, e para tudo de propósito.
    """
    try:
        return carregar(opcoes, roteiro)
    except Exception as erro:  # noqa: BLE001 — qualquer defeito vira motivo
        traceback.print_exc(file=sys.stdout)
        motivo = f"a carga caiu neste roteiro: {type(erro).__name__}: {erro}"
        print(motivo)
        return [motivo]


def varios(opcoes: argparse.Namespace, roteiros: list[pathlib.Path],
           carregar_um=None) -> int:
    """Todos os roteiros, cada um com a sua carga, e no fim quais reprovaram.

    **Todos, e não até o primeiro vermelho**: saber que dois quebraram é outra
    informação que saber que um quebrou. E o fim repete, numa linha por motivo,
    o roteiro e a frase com que ele reprovou: a bateria do `publicar.sh` mostra
    só as últimas linhas do que a etapa disse, e é ali que a frase tem de estar.
    """
    # **Dito antes de rodar, e sem esperar o buffer.** Um roteiro que pendura
    # não reprova: quem o para é o prazo de quem chamou (`timeout-minutes` no
    # CI, `PRAZO_DA_ETAPA_S` no `publicar.sh`), e a última linha escrita tem de
    # ser o nome dele. Com a saída num arquivo ou num cano, o Python guardaria
    # essa linha no buffer, e o `kill -9` do prazo a levaria junto.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(line_buffering=True)
    # Quem carrega cada roteiro: a carga de verdade, ou a de mentira do
    # autoteste, que mede a lista sem abrir navegador.
    carregar_um = carregar_um or carregar_sem_cair
    reprovados = []
    for roteiro in roteiros:
        print(f"roteiro: rodando {roteiro}")
        if roteiro.is_file():
            motivos = carregar_um(opcoes, roteiro)
        else:
            motivos = ["o arquivo do roteiro não existe"]
            print(motivos[0])
        if motivos:
            reprovados.append((roteiro, motivos))
        print()
    if not reprovados:
        print(f"roteiros: os {len(roteiros)} passaram.")
        return 0
    print(f"roteiros: {len(reprovados)} de {len(roteiros)} reprovaram:")
    for roteiro, motivos in reprovados:
        for motivo in motivos:
            print(f"  {roteiro}: {motivo}")
    return 1


def ler_saida(tudo: str) -> tuple[list[list[str]], list[str], list[str], list[str]]:
    """O que a página disse ao navegador: os relatos, as partes deles, os
    estouros e as falhas do roteiro (um REPROVADO, ou um roteiro que estourou).

    Uma função à parte para o autoteste ler, sem navegador, uma linha do console
    no formato do Chromium pelo mesmo código que lê a de verdade.
    """
    # `(in promise)` também: uma rejeição que ninguém pegou é um estouro, e
    # `desenharVersoes` estourou assim sem que este aparelho dissesse nada.
    estouros = sorted(set(re.findall(r"Uncaught (?:\(in promise\) )?[A-Za-z]*Error: [^\"]*", tudo)))
    # Cada relato é a lista que o roteiro relatou, em JSON e codificada para URL
    # (ver `PE_DO_ROTEIRO`): o código não tem aspa nem espaço, e o que o roteiro
    # escreveu volta inteiro, com aspas, quebras e « | » dentro.
    relatos = [json.loads(urllib.parse.unquote(codigo))
               for codigo in re.findall(r"SEELE-RELATO ([^\s\"]*)", tudo)]
    partes = [parte for relato in relatos for parte in relato]
    falhas = [p for p in partes if p.startswith(("REPROVADO", "ROTEIRO ESTOUROU"))]
    return relatos, partes, estouros, falhas


def autoteste() -> int:
    """Os consertos da carga, conferidos contra o código dela e sem navegador.

    M3 da revisão do lote RT: o relato que atravessa a aspa e os vários
    `--roteiro` numa chamada só se provavam por reversão à mão, e o segundo,
    revertido, deixaria o CI verde rodando um roteiro só. Cada caso abaixo
    reprova se o seu conserto for revertido (as reversões estão no relatório do
    lote). O `cargo test -p xtask` roda isto
    (`a_carga_da_casca_passa_no_proprio_autoteste`, em
    `xtask/src/check_runtime.rs`).
    """
    import contextlib
    import io
    import time

    roteiro_a = RAIZ / "tools" / "roteiros" / "perfil-entrada.js"
    roteiro_b = RAIZ / "tools" / "roteiros" / "perfil-sessao.js"

    def a_aspa() -> str | None:
        # A linha como o Chromium a escreve: entre aspas, sem escapar as de
        # dentro. O código é o que `PE_DO_ROTEIRO` produz: `JSON.stringify` e
        # `encodeURIComponent` (que deixa de fora `-_.!~*'()`).
        dito = ['com "aspas" aqui', "REPROVADO: depois da aspa"]
        codigo = urllib.parse.quote(json.dumps(dito, ensure_ascii=False, separators=(",", ":")),
                                    safe="-_.!~*'()")
        linha = (f'[1010/200000.000000:INFO:CONSOLE(4938)] "SEELE-RELATO {codigo}", '
                 "source: file:///tmp/x/ui/index.html (4938)")
        _, partes, _, falhas = ler_saida(linha)
        if partes != dito:
            return f"o relato voltou como {partes!r}, e não como {dito!r}"
        if falhas != ["REPROVADO: depois da aspa"]:
            return f"o REPROVADO depois da aspa não virou falha: {falhas!r}"
        return None

    def os_argumentos() -> str | None:
        opcoes = argumentos_da_carga().parse_args(
            ["--roteiro", str(roteiro_a), "--roteiro", str(roteiro_b)])
        if opcoes.roteiro != [roteiro_a, roteiro_b]:
            return f"dois --roteiro viraram {opcoes.roteiro!r}"
        return None

    def a_lista() -> str | None:
        rodados = []

        def de_mentira(_opcoes, roteiro):
            rodados.append(roteiro)
            return ["REPROVADO: o primeiro"] if roteiro == roteiro_a else []

        dito = io.StringIO()
        with contextlib.redirect_stdout(dito):
            codigo = varios(argparse.Namespace(), [roteiro_a, roteiro_b], de_mentira)
        if rodados != [roteiro_a, roteiro_b]:
            return f"a lista rodou {rodados!r}, e não os dois"
        if codigo != 1:
            return f"um roteiro reprovou e a lista saiu com {codigo}"
        if f"  {roteiro_a}: REPROVADO: o primeiro" not in dito.getvalue():
            return "o fim da lista não diz o roteiro que reprovou e a frase"
        return None

    def nao_cai() -> str | None:
        global carregar
        de_verdade = carregar

        def que_cai(_opcoes, _roteiro):
            raise RuntimeError("um defeito do aparelho")

        carregar = que_cai
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                motivos = carregar_sem_cair(argparse.Namespace(), roteiro_a)
        except Exception as erro:  # noqa: BLE001 — é o que se mede
            return f"a carga caiu: {type(erro).__name__}: {erro}"
        finally:
            carregar = de_verdade
        if motivos != ["a carga caiu neste roteiro: RuntimeError: um defeito do aparelho"]:
            return f"o defeito virou {motivos!r}"
        return None

    def o_prazo() -> str | None:
        # Um «navegador» que abre um filho, diz o número dele e dorme: o prazo
        # tem de matar os dois, como mata o renderer do de verdade.
        falso = ("import subprocess, sys, time; "
                 "filho = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); "
                 "print(filho.pid, flush=True); time.sleep(30)")
        comeco = time.monotonic()
        saida, venceu = rodar_navegador([sys.executable, "-c", falso], 1.0)
        gasto = time.monotonic() - comeco
        if not venceu:
            return f"o prazo de 1 s não venceu, e o «navegador» rodou {gasto:.1f} s"
        if gasto > 10:
            return (f"o prazo venceu, mas a carga esperou {gasto:.1f} s: um filho do "
                    "«navegador» continuou vivo e segurou a saída")
        filho = int(saida.split()[0])
        for _ in range(30):
            try:
                os.kill(filho, 0)
            except ProcessLookupError:
                return None
            time.sleep(0.1)
        os.kill(filho, signal.SIGKILL)
        return f"o filho do navegador ({filho}) continuou vivo depois do prazo"

    def o_sinal() -> str | None:
        # R2-M2: um SIGTERM no meio da espera, mandado pela própria carga a si
        # mesma 0,5 s depois de abrir o «navegador», tem de matar o grupo
        # dele antes de a carga sair. O «navegador» escreve num arquivo o
        # número dele e o do filho, porque a saída se perde com o sinal.
        with tempfile.TemporaryDirectory() as pasta:
            numeros = pathlib.Path(pasta) / "numeros"
            falso = ("import subprocess, sys, os, time; "
                     "filho = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); "
                     f"open({str(numeros)!r}, 'w').write(f'{{os.getpid()}} {{filho.pid}}'); "
                     "time.sleep(30)")
            anterior = signal.signal(signal.SIGALRM,
                                     lambda *_: os.kill(os.getpid(), signal.SIGTERM))
            signal.setitimer(signal.ITIMER_REAL, 0.5)
            try:
                rodar_navegador([sys.executable, "-c", falso], 30.0)
                return "o SIGTERM não interrompeu a espera pelo «navegador»"
            except SystemExit:
                pass
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, anterior)
            vivos = []
            for numero in numeros.read_text().split():
                for _ in range(30):
                    try:
                        os.kill(int(numero), 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.1)
                else:
                    vivos.append(numero)
                    os.kill(int(numero), signal.SIGKILL)
            if vivos:
                return f"depois do SIGTERM, continuaram vivos: {' '.join(vivos)}"
            return None

    casos = [
        ("o relato atravessa a aspa", a_aspa),
        ("vários --roteiro numa chamada", os_argumentos),
        ("a lista roda todos e diz quem reprovou", a_lista),
        ("a carga não cai", nao_cai),
        ("o prazo mata o navegador e os filhos", o_prazo),
        ("um SIGTERM no meio mata o navegador e os filhos", o_sinal),
    ]
    falharam = 0
    for nome, caso in casos:
        try:
            falha = caso()
        except Exception as erro:  # noqa: BLE001 — o caso que estoura reprova
            falha = f"estourou: {type(erro).__name__}: {erro}"
        if falha:
            falharam += 1
            print(f"autoteste: {nome}: FALHOU — {falha}")
        else:
            print(f"autoteste: {nome}: ok")
    if falharam:
        print(f"autoteste: {falharam} de {len(casos)} falharam")
        return 1
    print(f"autoteste: os {len(casos)} passaram")
    return 0


def carregar(opcoes: argparse.Namespace, roteiro_pedido: pathlib.Path | None) -> list[str]:
    """Uma carga, com um roteiro ou sem nenhum. Escreve o que viu e devolve os
    motivos da reprovação: os estouros e as falhas do roteiro, ou nada."""
    with tempfile.TemporaryDirectory() as temporaria:
        pasta = pathlib.Path(temporaria) / "ui"
        shutil.copytree(opcoes.casca, pasta)
        pagina = pasta / "index.html"
        texto = pagina.read_text()
        corte = texto.index("<script src=")
        roteiro = ""
        if roteiro_pedido:
            corpo = roteiro_pedido.read_text()
            roteiro = CABECA_DO_ROTEIRO + corpo + PE_DO_ROTEIRO
        duble = (
            DUBLE.replace("__APIS_ACEITAS__", apis_aceitas())
            .replace("__PERMISSOES_DA_JANELA__", permissoes_da_janela())
            .replace("__VERSAO_DO_SERVIDOR__", versao_do_servidor())
            .replace("__LINHA__", linha_desta_janela())
        )
        pagina.write_text(texto[:corte] + duble + texto[corte:] + roteiro)

        tudo, venceu = rodar_navegador(
            [
                navegador(),
                "--headless",
                "--disable-gpu",
                # **Como o Playwright o abre.** As bancadas do mesmo job do CI
                # rodam o Chromium pelo Playwright, que passa `--no-sandbox`
                # sempre que não se pede o contrário (`chromiumSandbox !== true`
                # no `playwright-core`). Este aparelho abre o navegador direto,
                # e o Ubuntu 24.04 restringe por AppArmor os espaços de nome
                # sem privilégio de que a caixa de areia do Chromium precisa: se
                # o runner do Actions mantém essa restrição não foi medido, e o
                # que a página carrega é uma cópia da casca deste repositório.
                "--no-sandbox",
                f"--window-size={opcoes.janela}",
                "--virtual-time-budget=5000",
                "--screenshot=" + str(pasta / "foto.png"),
                f"file://{pagina}",
            ],
            opcoes.prazo_por_roteiro,
        )

        if opcoes.foto and (pasta / "foto.png").exists():
            shutil.copy(pasta / "foto.png", opcoes.foto)

    relatos, partes, estouros, falhas = ler_saida(tudo)
    sem_resposta = sorted({c for linha in re.findall(r"SEELE-SEM-RESPOSTA ([^\"]*)", tudo)
                           for c in linha.split()})
    # Um roteiro que reprovou, que estourou no meio, ou que nem chegou ao fim
    # dentro do orçamento de tempo não é uma carga limpa — e as três coisas
    # terminavam com «carga limpa» antes.
    if venceu:
        falhas.append(f"o roteiro não terminou em {opcoes.prazo_por_roteiro:g} s, e o "
                      "navegador que a carga abriu foi morto com os filhos dele")
    elif roteiro_pedido and not relatos:
        falhas.append("o roteiro não chegou ao fim: nenhum relato saiu da página")

    if opcoes.tudo:
        for linha in tudo.splitlines():
            if "CONSOLE" in linha:
                print(linha.split("CONSOLE:", 1)[-1].strip())
    for parte in partes:
        print(parte)
    if sem_resposta:
        # Informação, e não reprovação: muitos destes respondem `null` no
        # produto também. Quando uma tela não troca, é aqui que se olha primeiro.
        print("\no duble respondeu null a: " + " ".join(sem_resposta))
    if estouros:
        print("\nestouros na carga:")
        for e in estouros:
            print(f"  {e}")
    if falhas:
        print("\no roteiro reprovou:")
        for f in falhas:
            print(f"  {f}")
    if estouros or falhas:
        return estouros + falhas
    print("\ncarga limpa: nenhum script estourou.")
    return []


if __name__ == "__main__":
    raise SystemExit(main())
