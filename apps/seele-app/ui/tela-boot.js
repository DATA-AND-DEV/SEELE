// SEELE · a tela de entrada (`#tela-boot`).
//
// Onde você já esteve, o convite colado, e a conexão — mais hospedar aqui
// dentro, que é a mesma conexão com um servidor que este processo acabou de
// subir. Sai daqui para `#tela-auth`, que é onde o veredito da chave é lido
// antes de se entrar em sala de voz nenhuma.
//
// A coluna da esquerda é a apresentação do produto e não tem código: marca,
// três fatos, glossário. Tudo o que este arquivo move está na direita, mais a
// linha de CONEXÃO embaixo da marca. Do que o comp v2 animava nesta tela, nada
// tinha dado por trás — o `bootPct` que subia de 7 em 7 era um cronômetro de
// protótipo, e o registro de dez linhas carimbadas descrevia um stream de
// progresso que o protocolo não tem. O que esta tela move é o que ela sabe: a
// tentativa de conexão, pelo tempo real dela.

"use strict";

/**
 * O estado da tentativa de conexão.
 *
 * Um estado, e não três. Aqui havia um laço sobre `sub-permissions`,
 * `sub-media` e `sub-persistence`, que escrevia **a mesma coisa** nos três — e o
 * comentário desta função dizia o motivo em voz alta: saúde por subsistema não
 * existe no protocolo, o fato relatado é um só. Três nomes para um dado é
 * cenário se passando por instrumento, e o laço era a prova disso escrita em
 * código.
 *
 * A marca de texto continua sendo o que diz qual estado é qual, porque a cor
 * sozinha não pode (`specs/06-clientes-gui.md`), e o movimento continua durando
 * o tempo real da conexão e parando com ela.
 */
function subsistemas(estado, marca) {
  const alvo = $("boot-tentativa-marca");
  alvo.textContent = marca;
  alvo.parentElement.dataset.estado = estado;
}

/** O convite lido do último `seele://` colado, se houver. */
let convitePendente = null;

/**
 * A lista de servidores onde esta pessoa já esteve.
 *
 * Quem já entrou uma vez não deveria ter que redigitar um endereço IP. A lista
 * chega pronta do Rust, do mais recente para o mais antigo — a única ordem útil
 * numa lista de atalhos.
 */
async function desenharVisitados() {
  const lista = await invoke("conhecidos");
  const secao = $("visitados");
  // Sem visitados, a seção some inteira: a tela volta a ser exatamente a de
  // antes desta seção existir, e o estado vazio não piora.
  secao.hidden = lista.length === 0;
  if (lista.length === 0) return;

  // O apelido da última visita vira o padrão do campo.
  //
  // `Conhecidos::listar` ordena do mais recente para o mais antigo, então o
  // primeiro é o nome com que esta pessoa entrou por último. Sem isto o campo
  // volta ao padrão da marcação a cada abertura do app — o nome estava gravado o tempo
  // todo, e a tela é que não o lia.
  //
  // Só quando o campo ainda está como a marcação o deixou. `defaultValue` é
  // exatamente essa pergunta, respondida pelo DOM: quem já digitou alguma coisa
  // não tem o que digitou trocado por baixo.
  const campo = $("campo-apelido");
  if (campo.value === campo.defaultValue) campo.value = lista[0].apelido;

  repovoar(
    $("lista-visitados"),
    lista.map((conhecido) => {
      const linha = elemento("li", "visitado");
      const ir = elemento("button", "visitado-ir", conhecido.alvo);
      ir.type = "button";
      ir.title = `entrar em ${conhecido.alvo} como ${conhecido.apelido}`;
      ir.addEventListener("click", () => {
        $("campo-servidor").value = conhecido.alvo;
        $("campo-apelido").value = conhecido.apelido;
        // Escolher da lista não é usar o convite colado.
        limparConvite();
        conectar();
      });
      const esquecer = elemento("button", "botao-fantasma", "esquecer");
      esquecer.type = "button";
      esquecer.addEventListener("click", async () => {
        try {
          await invoke("esquecer", { alvo: conhecido.alvo });
        } catch (falha) {
          // A lista não pôde ser reescrita — disco cheio, permissão. A linha
          // continua ali, e dizer isso é melhor que uma promessa recusada em
          // silêncio e uma linha que teima em voltar.
          console.warn("esquecer:", falha);
          const erro = $("boot-erro");
          erro.hidden = false;
          erro.textContent = "NÃO CONSEGUI REESCREVER A LISTA DE VISITADOS";
        }
        await desenharVisitados();
      });
      linha.append(
        ir,
        elemento("span", "visitado-apelido", conhecido.apelido),
        elemento("span", "visitado-quando", quando(conhecido.visto_em)),
        esquecer,
      );
      return linha;
    }),
  );
}

/**
 * Lê o `seele://` colado no campo CONVITE.
 *
 * Quem lê o link é o Rust: um segundo analisador aqui seria um segundo conjunto
 * de casos de borda para discordar do primeiro. A confirmação de identidade que
 * o link carrega fica lá também, guardada até o `connect` conferi-la — o que
 * volta para cá é o veredito, depois, e nunca o valor a comparar.
 */
async function lerConvite() {
  const campo = $("campo-convite");
  const link = campo.value.trim();
  const erro = $("boot-erro");
  if (link === "") {
    limparConvite();
    return;
  }

  try {
    const convite = await invoke("analisar_convite", { link });
    $("campo-servidor").value = convite.alvo;
    convitePendente = convite;
    erro.hidden = true;
  } catch (falha) {
    // O resto do formulário fica intacto: quem colou errado não perde o que já
    // tinha digitado nos outros campos.
    convitePendente = null;
    // Revelar antes de escrever: `#boot-erro` é `role="alert"`, e um alerta
    // escrito enquanto ainda está escondido não é anunciado por leitor de tela.
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
}

/**
 * Esquece o convite colado, campo e tudo.
 *
 * O token vale para o servidor daquele link. Deixá-lo para trás numa troca de
 * endereço manda a credencial de um servidor para outro, que a recusa — e a
 * recusa aparece como "credencial rejeitada" num servidor que nunca pediu
 * credencial nenhuma.
 */
function limparConvite() {
  $("campo-convite").value = "";
  convitePendente = null;
}

async function conectar(evento) {
  evento?.preventDefault();
  const botao = $("botao-conectar");
  const erro = $("boot-erro");

  botao.disabled = true;
  erro.hidden = true;
  // A linha de CONEXÃO reporta enquanto a conexão acontece. Dura o tempo real
  // dela: `specs/06-clientes-gui.md` chama animação decorativa que atrasa o
  // usuário de falha de design.
  subsistemas("carga", "…");
  // A linha de etapa nasce vazia a cada tentativa: a de uma conexão anterior
  // descreveria uma travessia que já acabou.
  mostrarEtapa(null);

  try {
    // A entrada traz duas coisas: a tela, e o que a chave deste servidor acabou
    // de ser. A segunda vem do mesmo `connect` porque é lá que ela é decidida —
    // um ouvinte inscrito depois chegaria sempre tarde.
    const { snapshot, veredito } = await invoke("connect", {
      server: $("campo-servidor").value.trim(),
      nickname: $("campo-apelido").value.trim(),
      audio: $("campo-audio").checked,
      // O token do convite, quando o link trouxe um. `join_secret` do outro
      // lado: a ponte do Tauri converte para camelCase. A confirmação de
      // identidade do mesmo link não passa por aqui: ela ficou no Rust, que é
      // quem confere.
      joinSecret: convitePendente?.token ?? null,
    });

    subsistemas("ok", "ok");

    // Daqui em diante quem manda é `#tela-auth`: o veredito da chave é lido
    // numa tela antes de se entrar em sala de voz nenhuma, e a entrada mudou de
    // lugar junto com ele. Enquanto isso não acontece, a sessão continua
    // desenhada com o que o `connect` já devolveu — quem chegar nela não espera
    // o primeiro tique do laço de snapshot.
    desenhar(snapshot);
    entrarNaAutenticacao(snapshot, veredito, $("campo-servidor").value.trim());
  } catch (falha) {
    subsistemas("", "·");
    // `connect` responde por `ConnectFailure` desde esta tarefa: o erro de
    // sempre **mais a trilha**. Quem escreve a frase quer o erro; a trilha vai
    // para o console, que é onde alguém que está investigando a procura — «o
    // primeiro deu prazo esgotado em 4 s, o quarto recusou» é o dado que faltou
    // quando o teste de campo das duas casas falhou.
    //
    // O `?? falha` não é zelo: `analisar_convite` e o `AlreadyConnected` deste
    // mesmo comando respondem com o enum cru, e a mesma função lê os dois.
    const motivo = falha?.error ?? falha;
    if (Array.isArray(falha?.trail) && falha.trail.length > 0) {
      console.warn("chegada:", falha.trail);
    }
    // Uma batida que ficou pendente na portaria tem tela própria (ADR 0030), e
    // uma falha que chega enquanto essa tela está na frente pertence a ela:
    // `#boot-erro` estaria escondido atrás. `levarParaAEspera` responde se
    // tratou a falha, e só o que sobra vira a linha vermelha daqui.
    if (!levarParaAEspera(motivo, $("campo-servidor").value.trim())) {
      erro.hidden = false;
      erro.textContent = fraseDeErro(motivo);
    }
  } finally {
    botao.disabled = false;
    // A travessia acabou, de um jeito ou de outro. O que sobra na tela é o
    // veredito ou o erro, e não o último candidato tentado.
    mostrarEtapa(null);
  }
}

/**
 * Escreve onde a chegada está, ou apaga a linha.
 *
 * `null` esconde: uma linha vazia e visível empurra o formulário para baixo a
 * cada tentativa, e um `role="status"` vazio é anunciado como nada.
 */
function mostrarEtapa(frase) {
  const linha = $("boot-etapa");
  if (frase === null) {
    linha.hidden = true;
    linha.textContent = "";
    return;
  }
  // Visível primeiro, escrita depois: um `role="status"` escondido no instante
  // em que o texto muda não é anunciado. É a mesma ordem da faixa de veredito.
  linha.hidden = false;
  linha.textContent = frase;
}

// As etapas da chegada, enquanto ela acontece.
//
// Chegam pelo mesmo canal de sempre, e são o único evento da lista que existe
// **antes** de haver sessão: a FFI recebe a ponte antes de bloquear no aperto
// de mão (`Connection::connect_watching`). Sem isto o spinner desta tela era mudo, e
// quando o teste de campo das duas casas falhou ninguém soube dizer em que
// ponto — quatro candidatos eram tentados em série atrás dos três blocos.
listen("seele://event", (evento) => {
  const payload = evento.payload;
  if (!payload || typeof payload !== "object" || !payload.ConnectStageChanged) return;
  // Só com a tela de entrada na frente. Uma reconexão futura que publicasse
  // etapas escreveria numa tela escondida atrás da sessão.
  if ($("tela-boot").hidden) return;
  mostrarEtapa(fraseDeEtapa(payload.ConnectStageChanged.stage));
});

/**
 * Diz até onde o link recém-criado chega, embaixo dele.
 *
 * O `alcance` é o degrau da escada do ADR 0022 em que o servidor parou, como
 * nome estável — `PortaNoRoteador`, `FuroDeNat`, `Ipv6Direto`, `RedeLocalOuVpn`
 * ou `SoRedeLocal` —, e a frase mora no `FRASES`, que é onde moram todas.
 *
 * Só os degraus que **não** alcançam de fora ganham destaque, e são os únicos
 * que precisam: os outros são boas notícias, e uma boa notícia gritada vira
 * ruído que se aprende a ignorar — inclusive no dia em que a notícia for ruim.
 *
 * `RedeLocalOuVpn` conta como perto: quem hospeda com uma VPN de navegação
 * ligada tem um endereço que parece alcançar o mundo e não aceita ninguém.
 */
function mostrarAlcance(alcance, portaRecusada, encontroRecusado) {
  const onde = $("convite-alcance");
  const frase = fraseDeErro(alcance);
  const soPerto = alcance === "SoRedeLocal" || alcance === "RedeLocalOuVpn";

  onde.textContent = frase;
  onde.classList.toggle("convite-alcance-curto", soPerto);
  onde.classList.toggle("convite-alcance-longe", !soPerto);

  // Por que cada degrau de cima não deu, quando chegou a ser tentado. Vai
  // embaixo e menor: é a pista de quem for investigar, não a mensagem de quem
  // só quer mandar o link. São duas linhas e não uma porque numa casa em que
  // nem o roteador abriu a porta nem o ponto de encontro respondeu são **duas**
  // informações, e quem for investigar precisa das duas.
  //
  // **Sem rótulo colado na frente**, e isto foi conserto: cada frase já se
  // nomeia — as quatro do roteador dizem «roteador» e as do encontro dizem
  // «ponto de encontro». Um `o roteador respondeu:` aqui produziu, numa tela de
  // verdade, «o roteador respondeu: o roteador respondeu, e o endereço dele…».
  // O rótulo só parecia necessário porque quem o escreveu não estava lendo a
  // frase que ele ia prefixar.
  for (const motivo of [portaRecusada, encontroRecusado]) {
    if (!motivo) continue;
    const detalhe = document.createElement("span");
    detalhe.className = "convite-alcance-detalhe";
    detalhe.textContent = motivo;
    onde.append(detalhe);
  }

  onde.hidden = false;
}

/**
 * Vira anfitrião: sobe o servidor dentro deste app e entra nele.
 *
 * Duas etapas de propósito. `hospedar` põe o servidor de pé e devolve o link;
 * conectar é o caminho de sempre, com o endereço que ele devolveu. Um servidor
 * hospedado aqui e um do outro lado do mundo entram pela mesma porta.
 */
async function hospedar() {
  const botao = $("botao-hospedar");
  const erro = $("boot-erro");
  botao.disabled = true;
  erro.hidden = true;

  try {
    const anfitriao = await invoke("hospedar");
    $("campo-servidor").value = anfitriao.aqui;
    $("convite-link").value = anfitriao.convite;
    mostrarAlcance(
      anfitriao.alcance,
      anfitriao.porta_recusada,
      anfitriao.encontro_recusado,
    );
    $("convite").hidden = false;

    // **O link, guardado e mostrado.** A comp da 0.9.0 promove a um diálogo o
    // que era um bloco entre outros nesta tela: quem acabou de hospedar precisa
    // de uma coisa só — o link — e precisa dela agora. Um bloco é uma coisa que
    // se acha; um diálogo é uma coisa que chega.
    //
    // Guardado antes de mostrado porque a configuração é onde ele mora depois,
    // e o diálogo promete isso por escrito. Um diálogo que prometesse um lugar
    // vazio seria pior que não prometer nada.
    guardarOLinkDaPorta(anfitriao.convite);
    abrirPorta($("convite-alcance").textContent);

    await conectar();
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
}

// ------------------------------------------------------------------- ligação

$("form-conectar").addEventListener("submit", conectar);
$("campo-convite").addEventListener("change", lerConvite);
// `paste` dispara antes de o valor entrar no campo; o tique seguinte já o tem.
$("campo-convite").addEventListener("paste", () => setTimeout(lerConvite, 0));

// Digitar outro endereço à mão desfaz o convite. `lerConvite` escreve neste
// campo por código, e atribuição não dispara `input` — só o teclado chega aqui.
$("campo-servidor").addEventListener("input", limparConvite);

$("botao-hospedar").addEventListener("click", hospedar);

// A tela de entrada é a primeira coisa que aparece, e a lista faz parte dela.
desenharVisitados().catch((falha) => console.warn("conhecidos:", falha));

// ------------------------------------------------- o microfone, antes de entrar
//
// Uma consulta ao sistema, no arranque e a cada volta para esta tela. Não pede
// nada e não muda nada: só lê o que o Windows já decidiu. Ver
// `seele_audio::device::consentimento_do_microfone`.
//
// Aqui e não na sessão porque é aqui que dá para agir a tempo: quem descobre
// que está mudo lá dentro já falou por cinco minutos para uma sala calada. Foi
// exatamente o que aconteceu num teste de campo, e a única coisa que a tela
// daquela pessoa dizia era «SEM ÁUDIO».

/** Mostra o bloqueio do microfone, ou esconde quando não há o que dizer. */
async function conferirMicrofone() {
  const bloco = $("boot-microfone");
  if (!bloco) return;
  let permissao = "NaoSeSabe";
  try {
    permissao = await invoke("permissao_de_microfone");
  } catch (falha) {
    console.warn("permissao_de_microfone:", falha);
  }
  const dito = PERMISSAO_DE_MICROFONE[permissao];
  bloco.hidden = !dito;
  if (!dito) return;
  $("boot-microfone-diz").textContent = dito.diz;
  $("boot-microfone-nota").textContent = dito.nota;
}

$("boot-microfone-ajustes").addEventListener("click", () => {
  invoke("abrir_ajustes_do_microfone").catch((falha) =>
    console.warn("abrir_ajustes_do_microfone:", falha),
  );
});

conferirMicrofone().catch((falha) => console.warn("microfone:", falha));
