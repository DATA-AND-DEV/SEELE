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
(() => {
  const campo = document.getElementById("convite");
  const testar = document.getElementById("testar");
  const parar = document.getElementById("parar");
  const estado = document.getElementById("estado");
  const resultado = document.getElementById("resultado");
  const copiar = document.getElementById("copiar");
  const entrar = document.getElementById("entrar");
  let ativo = null, geracao = 0, pararMedidas = () => {};
  const pedida = Number(new URLSearchParams(location.search).get("familia"));
  const familia = pedida === 4 || pedida === 6 ? pedida : null;
  if (familia) testar.textContent = `Testar IPv${familia}`;
  const fragmento = location.hash;
  if (fragmento) {
    campo.value = `https://seele.sh/${fragmento}`;
    history.replaceState(null, "", location.pathname + location.search);
  }
  function lerConvite(texto) {
    const url = new URL(texto.trim());
    if (url.protocol !== "https:") throw new Error("Cole o convite HTTPS do SEELE.");
    const hash = url.hash.slice(1), corte = hash.indexOf("?");
    if (corte < 0) throw new Error("Convite incompleto.");
    const dados = new URLSearchParams(hash.slice(corte + 1));
    const impressao = dados.get("fp");
    const enc = dados.get("enc"), barra = enc?.indexOf("/");
    if (!/^[a-f0-9]{64}$/i.test(impressao || "") || !enc || barra < 1) throw new Error("Convite sem impressão ou ponto de encontro.");
    const ponto = enc.slice(0, barra);
    // Este diagnóstico só usa o encontro oficial; nunca faz POST para um
    // destino arbitrário embutido em texto colado.
    if (ponto !== "rendzvous.seele.sh:8384") throw new Error("Este teste requer o ponto de encontro oficial.");
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
    const vez = ++geracao, inicio = performance.now();
    testar.disabled = true; parar.disabled = false; copiar.disabled = true; entrar.hidden = true;
    resultado.textContent = ""; estado.textContent = "Verificando conexão…";
    const relatorio = { teste: "servidor-hospedado-3", horario: new Date().toISOString(), etapas: [] };
    let pararEstaMedicao = () => {};
    let enlace;
    let sinalFiltrado;
    try {
      const convite = lerConvite(campo.value);
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
      if (vez === geracao) estado.textContent = "A conexão não abriu. Copie o diagnóstico abaixo.";
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
        resultado.textContent = JSON.stringify(relatorio, null, 2); copiar.disabled = false;
      }
    }
  });
  copiar.addEventListener("click", async () => {
    try { await navigator.clipboard.writeText(resultado.textContent); estado.textContent = "Diagnóstico copiado."; }
    catch { estado.textContent = "Selecione e copie o diagnóstico abaixo."; }
  });
  window.addEventListener("pagehide", () => { geracao++; pararMedidas(); ativo?.close(); ativo = null; });
})();
