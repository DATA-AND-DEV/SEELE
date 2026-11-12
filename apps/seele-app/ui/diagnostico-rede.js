// Prova somente do transporte até a apresentação do servidor. Não autentica
// uma pessoa, não entra em salas e nunca coleta mídia nem envia o diagnóstico.
// O filtro só pertence ao diagnóstico; não altera a negociação do aplicativo.
function sinalDaFamilia(sinal, familia) {
  const enviados = new Set();
  const aceita = (alvo) => separarAlvo(alvo, 8383)?.familia === familia;
  return {
    enviados,
    aceita,
    montar(pedido) {
      const candidatos = pedido.candidatos.filter(aceita);
      if (!candidatos.length) return { erro: `SemCandidatoIPv${familia}` };
      const montado = sinal.montar({ ...pedido, candidatos });
      if (montado.corpo) candidatos.forEach(c => enviados.add(c));
      return montado;
    },
    ler(texto) {
      const lido = sinal.ler(texto);
      if (!lido.aqui) return lido;
      const alvos = [lido.aqui.endereco, ...(lido.aqui.dicas || [])].filter(aceita);
      if (!alvos.length) return { semFamilia: true };
      return { ...lido, aqui: { ...lido.aqui, endereco: alvos[0], dicas: alvos.slice(1) } };
    },
  };
}
/**
 * **As falhas de conexão que a página aceita anotar**, vindas no `?erro=` de
 * quem chegou pelo TESTAR A CONEXÃO de uma tela do SEELE. Lista fechada, e a
 * mesma de `FALHAS_QUE_OFERECEM_O_TESTE_DE_CONEXAO` (`frases.js`; a bancada
 * `teste-de-conexao.cjs` confere as duas): o que não está aqui não entra no
 * texto copiado. Um endereço alterado por alguém não põe palavra nenhuma no
 * diagnóstico de outra pessoa.
 */
const ERROS_QUE_A_PAGINA_ANOTA = Object.freeze([
  "Unreachable",
  "SemResposta",
  "NatNaoAbriu",
  "LinkLost",
  "Timeout",
  "SemRefletido",
  "SinalMudo",
  "CaminhoNaoAbriu",
]);

/** O nome da falha que trouxe a pessoa aqui, ou `null`. */
function erroVistoNaConsulta(consulta) {
  const nome = new URLSearchParams(consulta).get("erro");
  return ERROS_QUE_A_PAGINA_ANOTA.includes(nome) ? nome : null;
}

/**
 * O convite colado, como o corpo de um `seele://`: o fragmento do link do
 * navegador (`https://…/#corpo`) ou o que vem depois do esquema do convite do
 * app. Os dois são o mesmo texto (`Convite::no_navegador`, em `uri.rs`).
 */
function corpoDoConvite(texto) {
  const colado = texto.trim();
  if (!colado) throw new Error("Cole o convite do servidor antes de testar.");
  // O `seele://máquina:porta/?…` com a barra antes da consulta é o mesmo
  // convite (`uri.rs` lê os dois), e o fragmento do navegador não a tem.
  if (/^seele:\/\//i.test(colado)) return colado.slice("seele://".length).replace(/\/(?=\?)/, "");
  let url;
  try {
    url = new URL(colado);
  } catch {
    throw new Error("Isto não é um convite do SEELE. Cole o link inteiro, como você o recebeu.");
  }
  if (url.protocol !== "https:") throw new Error("Isto não é um convite do SEELE. Cole o link inteiro, como você o recebeu.");
  return url.hash.slice(1);
}

/**
 * **O texto curto que o botão copia**: o resultado da medida e a falha que a
 * pessoa viu, em linhas que cabem numa mensagem.
 *
 * **Só o que o relatório já tem**: estados, tipos, famílias e contagens. Nada
 * de endereço, porta, convite, impressão, apelido ou nome de servidor: o
 * relatório não os guarda (`resumirCaminhoRtc`), e este texto só lê dele.
 *
 * **Ausência não vira zero.** Uma contagem que o navegador não deu sai como
 * «não medido», e uma soma só soma o que foi medido: zero é uma medida, e
 * quem lê «0 respostas» conclui outra coisa de quem lê «não medido».
 */
function resumoDoDiagnostico(relatorio, erroVisto) {
  const dito = (valor) => (valor === null || valor === undefined || valor === "" ? "não medido" : String(valor));
  const contagem = (tabela) => {
    const partes = Object.entries(tabela || {}).map(([tipo, quantos]) => `${tipo} ${quantos}`);
    return partes.length ? partes.join(", ") : "nenhum";
  };
  const soma = (pares, campo) => {
    const medidos = pares.map((par) => par[campo]).filter((n) => typeof n === "number");
    return medidos.length ? medidos.reduce((a, b) => a + b, 0) : "não medido";
  };
  const linhas = [
    "SEELE · teste de conexão",
    `feito em ${dito(relatorio.horario)} · ${dito(relatorio.teste)}${relatorio.familia ? ` · só ${relatorio.familia}` : ""}`,
    `falha vista no SEELE: ${erroVisto ?? "não informada"}`,
    relatorio.conectou
      ? `resultado: conectou em ${dito(relatorio.duracaoMs)} ms`
      : `resultado: não conectou (${dito(relatorio.motivo)}) em ${dito(relatorio.duracaoMs)} ms`,
  ];
  const etapas = (relatorio.etapas || []).map((e) => `${e.etapa} ${e.ms} ms`);
  linhas.push(`etapas: ${etapas.length ? etapas.join(" > ") : "nenhuma"}`);
  linhas.push(
    `ponto de encontro: ${dito(relatorio.sinal)} · chamadas ${dito(relatorio.anuncios)} · ` +
    `endereço refletido ${relatorio.refletido === undefined ? "não medido" : relatorio.refletido ? "sim" : "não"}`,
  );
  linhas.push(
    `endereços: enviados ${dito(relatorio.enderecosEnviados)} · do servidor ${dito(relatorio.enderecosDoServidor)} · ` +
    `recusados ${dito(relatorio.candidatosRecusados)}`,
  );
  const antes = relatorio.antesDeFechar || relatorio.caminho?.estado || null;
  linhas.push(
    `estado: ice ${dito(antes?.ice ?? relatorio.ice)} · conexão ${dito(antes?.conexao ?? relatorio.conexao)} · ` +
    `dtls ${dito(relatorio.dtls ?? antes?.dtls)}`,
  );
  const caminho = relatorio.caminho;
  if (!caminho || caminho.indisponivel) {
    linhas.push("caminho: não medido");
  } else {
    const pares = caminho.pares || [];
    const estados = {};
    for (const par of pares) estados[par.estado] = (estados[par.estado] || 0) + 1;
    linhas.push(`candidatos deste lado: ${contagem(caminho.locais)}`);
    linhas.push(`candidatos do servidor: ${contagem(caminho.remotos)}`);
    linhas.push(
      `pares: ${pares.length}${pares.length ? ` (${contagem(estados)})` : ""} · ` +
      `pedidos enviados ${soma(pares, "pedidosEnviados")} · respostas recebidas ${soma(pares, "respostasRecebidas")}`,
    );
  }
  linhas.push("Sem endereço, convite, chave nem nome: só estados, tipos e contagens.");
  return linhas.join("\n");
}
(() => {
  const campo = document.getElementById("convite");
  const testar = document.getElementById("testar");
  const parar = document.getElementById("parar");
  const estado = document.getElementById("estado");
  const resultado = document.getElementById("resultado");
  const copiar = document.getElementById("copiar");
  const entrar = document.getElementById("entrar");
  const etapas = document.getElementById("etapas");
  const resumo = document.getElementById("resumo");
  const copia = document.getElementById("copia");
  const visto = document.getElementById("visto");
  // A falha que trouxe a pessoa aqui (o TESTAR A CONEXÃO de uma tela do SEELE).
  // Dita na tela antes de qualquer teste: ela vai no texto copiado, e quem
  // copia tem de saber o que está mandando.
  const erroVisto = erroVistoNaConsulta(location.search);
  if (erroVisto) {
    visto.textContent = `Você veio de uma falha de conexão do SEELE (${erroVisto}). Ela vai junto no diagnóstico copiado.`;
    visto.hidden = false;
  }
  /** O resultado em linhas (o canvas): uma por etapa medida, e o desfecho. O texto que se copia é o resumo, logo abaixo; o JSON completo fica no fim, para quem o pedir. */
  function desenharAsEtapas(relatorio) {
    const linha = (marca, estado, texto, valor) => {
      const li = document.createElement("li");
      li.dataset.estado = estado;
      const m = document.createElement("span"); m.setAttribute("aria-hidden", "true"); m.textContent = marca;
      const t = document.createElement("span"); t.textContent = texto;
      li.append(m, t);
      if (valor) { const v = document.createElement("span"); v.textContent = valor; li.append(v); }
      return li;
    };
    const linhas = (relatorio.etapas || []).map(e => linha("\u2713", "ok", e.etapa, `${e.ms}ms`));
    linhas.push(relatorio.conectou
      ? linha("\u2713", "ok", "conectou", `${relatorio.duracaoMs}ms`)
      : linha("!", "falha", "não conectou", relatorio.motivo || ""));
    etapas.replaceChildren(...linhas);
    etapas.hidden = false;
  }
  let ativo = null, geracao = 0, pararMedidas = () => {};
  const pedida = Number(new URLSearchParams(location.search).get("familia"));
  const familia = pedida === 4 || pedida === 6 ? pedida : null;
  if (familia) testar.textContent = `Testar IPv${familia}`;
  const fragmento = location.hash;
  if (fragmento) {
    // A página veio do link: a origem dela é a do convite.
    campo.value = `https://${location.host}/${fragmento}`;
    history.replaceState(null, "", location.pathname + location.search);
  }
  function lerConvite(texto) {
    const hash = corpoDoConvite(texto), corte = hash.indexOf("?");
    if (corte < 0) throw new Error("Convite incompleto: falta a parte depois do endereço. Cole o link inteiro.");
    const dados = new URLSearchParams(hash.slice(corte + 1));
    const impressao = dados.get("fp");
    const enc = dados.get("enc"), barra = enc?.indexOf("/");
    if (!/^[a-f0-9]{64}$/i.test(impressao || "") || !enc || barra < 1) throw new Error("Este convite não traz a impressão do servidor ou o ponto de encontro, e o teste mede por eles.");
    const ponto = enc.slice(0, barra);
    // Este diagnóstico só usa o encontro oficial; nunca faz POST para um
    // destino arbitrário embutido em texto colado.
    if (ponto !== "rendzvous.seele.sh:8384") throw new Error("Este servidor usa outro ponto de encontro, e o teste só mede pelo oficial.");
    const alvos = [hash.slice(0, corte), ...(dados.get("alt") || "").split(","), enc.slice(barra + 1)].filter(Boolean);
    return { hash, alvos: [...new Set(alvos)], impressao, bilhete: { ponto } };
  }
  function acompanhar(enlace, relatorio, vez, inicio) {
    let parado = false, relogio;
    async function medir() {
      if (parado || vez !== geracao) return;
      const amostra = await enlace.medirCaminho();
      if (parado || vez !== geracao) return;
      // Uma coleta depois do fechamento pode vir vazia: não apagar a última útil.
      if (!relatorio.caminho || amostra.pares?.length) {
        relatorio.caminho = { ms: Math.round(performance.now() - inicio), ...amostra };
      }
      relogio = setTimeout(medir, 300);
    }
    medir().catch(() => {});
    return () => { parado = true; clearTimeout(relogio); };
  }
  parar.addEventListener("click", () => { geracao++; pararMedidas(); ativo?.close(); ativo = null; testar.disabled = false; parar.disabled = true; estado.textContent = "Teste cancelado."; });
  testar.addEventListener("click", async () => {
    // **O convite é lido antes de o teste começar, e o que falta nele é dito.**
    // A leitura morava dentro do `try` da medida, e um convite que não servia
    // (vazio, cortado, de outro ponto de encontro) acabava em «A conexão não
    // abriu. Copie o diagnóstico abaixo», com um relatório de motivo `Error`:
    // a página sabia o que faltava e mandava copiar uma medida que não houve.
    let convite;
    try {
      convite = lerConvite(campo.value);
    } catch (recusa) {
      geracao++; pararMedidas(); ativo?.close(); ativo = null;
      campo.setAttribute("aria-invalid", "true");
      testar.disabled = false; parar.disabled = true; copiar.disabled = true; entrar.hidden = true;
      resultado.textContent = ""; resumo.textContent = ""; copia.textContent = ""; delete copia.dataset.tipo;
      etapas.replaceChildren(); etapas.hidden = true;
      estado.textContent = `${recusa.message} Nada foi testado.`;
      return;
    }
    campo.removeAttribute("aria-invalid");
    const vez = ++geracao, inicio = performance.now();
    testar.disabled = true; parar.disabled = false; copiar.disabled = true; entrar.hidden = true;
    resultado.textContent = ""; resumo.textContent = ""; copia.textContent = ""; delete copia.dataset.tipo;
    etapas.replaceChildren(); etapas.hidden = true; estado.textContent = "Verificando conexão…";
    const relatorio = { teste: "servidor-hospedado-3", horario: new Date().toISOString(), etapas: [] };
    let pararEstaMedicao = () => {};
    let enlace;
    let sinalFiltrado;
    try {
      const bytes = await (await fetch("seele_web.wasm", { cache: "no-cache" })).arrayBuffer();
      const wasm = (await WebAssembly.instantiate(bytes, {})).instance.exports;
      const certificado = await prepararCertificadoDtls();
      if (vez !== geracao) return;
      const sinal = fronteiraDoSinal(wasm);
      sinalFiltrado = familia ? sinalDaFamilia(sinal, familia) : null;
      const alvos = sinalFiltrado ? convite.alvos.filter(sinalFiltrado.aceita) : convite.alvos;
      if (familia) {
        relatorio.familia = `IPv${familia}`;
        relatorio.alvosDaFamilia = alvos.length;
      }
      if (!alvos.length) throw new Error("Convite sem endereço desta família.");
      enlace = ativo = criarEnlaceRtc({ ...convite, alvos, certificado, sinal: sinalFiltrado || sinal,
        aoMudar(e) { relatorio.etapas.push({ etapa: Object.keys(e)[0], ms: Math.round(performance.now() - inicio) }); },
      });
      pararEstaMedicao = pararMedidas = acompanhar(enlace, relatorio, vez, inicio);
      await enlace.ready;
      relatorio.conectou = true;
      if (vez === geracao) {
        estado.textContent = "Conexão direta com o servidor confirmada.";
        entrar.href = `/${"#" + convite.hash}`; entrar.hidden = false;
      }
    } catch (e) {
      relatorio.conectou = false;
      relatorio.motivo = e?.FalhaDoEnlace?.motivo || (e instanceof Error ? e.name : "Falha");
      relatorio.ice = e?.FalhaDoEnlace?.detalhe?.ice || null;
      relatorio.conexao = e?.FalhaDoEnlace?.detalhe?.conexao || null;
      if (vez === geracao) estado.textContent = "A conexão não abriu. O diagnóstico está abaixo, pronto para copiar.";
    } finally {
      pararEstaMedicao();
      const v = enlace?.visto;
      if (v) {
        // Somente contagens/estados. Sem IPs, SDP, convite, credenciais ou nomes.
        const s = v.sinal || {};
        relatorio.refletido = Boolean(v.refletido);
        relatorio.sinal = ["aqui", "ninguem", "calado", "recusa", "mudo", "semSinal", "semFamilia", "erro", "pendente"].find(k => s[k]) || "nao-iniciado";
        relatorio.dtls = v.dtls || null;
        relatorio.anuncios = v.chamadasAoPonto || 0;
        relatorio.enderecosEnviados = sinalFiltrado ? sinalFiltrado.enviados.size : v.candidatosAnunciados || 0;
        relatorio.enderecosDoServidor = v.candidatos.length;
        relatorio.antesDeFechar = v.antesDeFechar || null;
        relatorio.candidatosRecusados = enlace.contas.candidatosRecusados;
      }
      relatorio.duracaoMs = Math.round(performance.now() - inicio);
      enlace?.close();
      if (vez === geracao) {
        ativo = null; testar.disabled = false; parar.disabled = true;
        // A falha que a pessoa viu entra no relatório também: o JSON e o resumo dizem o mesmo.
        relatorio.erroVisto = erroVisto;
        resultado.textContent = JSON.stringify(relatorio, null, 2);
        // O resumo fica à vista: o que o botão copia é o que está escrito aqui.
        resumo.textContent = resumoDoDiagnostico(relatorio, erroVisto); copiar.disabled = false;
        desenharAsEtapas(relatorio);
      }
    }
  });
  // **A cópia diz o que copiou, e diz quando não copiou.** O texto é o do
  // quadro acima do botão, inteiro e só ele: quem cola numa conversa manda o
  // que leu aqui. Uma área de transferência recusada (permissão negada, página
  // sem foco, navegador sem a API) era um «Selecione e copie» que não dizia que
  // a cópia tinha falhado: agora a frase diz, e o texto já fica selecionado.
  copiar.addEventListener("click", async () => {
    const texto = resumo.textContent;
    if (!texto) return;
    copia.textContent = ""; delete copia.dataset.tipo;
    try {
      await navigator.clipboard.writeText(texto);
      copia.textContent = "Copiado: o texto do quadro acima, inteiro e só ele. Cole na conversa com quem hospeda.";
      // Inteira na tela: num celular o botão fica no fim da janela, e a frase nasceria abaixo dela.
      copia.scrollIntoView({ block: "nearest" });
    } catch {
      copia.dataset.tipo = "erro";
      copia.textContent = "Não foi copiado: o navegador recusou a área de transferência. O texto do quadro acima ficou selecionado: copie com Ctrl+C, ou com o toque longo.";
      const faixa = document.createRange();
      faixa.selectNodeContents(resumo);
      const selecao = getSelection();
      selecao.removeAllRanges();
      selecao.addRange(faixa);
      copia.scrollIntoView({ block: "nearest" });
    }
  });
  window.addEventListener("pagehide", () => { geracao++; pararMedidas(); ativo?.close(); ativo = null; });
})();
