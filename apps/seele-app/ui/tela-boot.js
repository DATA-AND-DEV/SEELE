// SEELE · Entry Plug — a tela de entrada (`#tela-boot`).
//
// Onde você já esteve, o convite colado, e a conexão — mais hospedar aqui
// dentro, que é a mesma conexão com um Dogma que este processo acabou de subir.
// Sai daqui para `#tela-auth`, que é onde o veredito da chave é lido antes de o
// plug entrar em Cage nenhum.
//
// A tela é a transcrição dos painéis A·01 e B·01 do comp v2: a ficha da
// máquina, o diagrama dos três subsistemas, a Taxa de Sincronização e o registro
// de carga. Do que o comp anima aqui, nada tem dado por trás — o `bootPct` que
// sobe de 7 em 7 é um cronômetro de protótipo, e o registro de dez linhas
// carimbadas descreve um stream de progresso que o protocolo não tem. O que
// esta tela move é o que ela realmente sabe: os três blocos, durante a
// tentativa de conexão, pelo tempo real dela.

"use strict";

/**
 * O estado dos três blocos MAGI.
 *
 * O que eles relatam é **a tentativa de conexão**, e não saúde por subsistema —
 * essa não existe em lugar nenhum do protocolo (§16 do inventário do comp), e é
 * por isso que os três mudam juntos: o fato é um só. A marca de texto dentro de
 * cada bloco é o que diz qual estado é qual, porque a cor sozinha não pode
 * (`specs/05-cliente-tui.md`), e `specs/07-tema-evangelion.md` só admite
 * movimento que diagnostique — este dura o tempo real da conexão e para com ela.
 */
function subsistemas(estado, marca) {
  for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) {
    const alvo = $(id);
    alvo.textContent = marca;
    alvo.parentElement.dataset.estado = estado;
  }
}

/** O convite lido do último `seele://` colado, se houver. */
let convitePendente = null;

/**
 * A lista de Dogmas onde este piloto já esteve.
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
  // volta a `piloto` a cada abertura do app — o nome estava gravado o tempo
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
 * O token vale para o Dogma daquele link. Deixá-lo para trás numa troca de
 * endereço manda a credencial de um servidor para outro, que a recusa — e a
 * recusa aparece como "credencial rejeitada" num Dogma que nunca pediu
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
  // Os três subsistemas reportam enquanto a conexão acontece. Duram o tempo
  // real dela: `specs/05-cliente-tui.md` chama animação decorativa que atrasa
  // o usuário de falha de design.
  subsistemas("carga", "…");

  try {
    // A entrada traz duas coisas: a tela, e o que a chave deste Dogma acabou
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
    // numa tela antes de o plug entrar em Cage nenhum, e a inserção mudou de
    // lugar junto com ele. Enquanto isso não acontece, a sessão continua
    // desenhada com o que o `connect` já devolveu — quem chegar nela não espera
    // o primeiro tique do laço de snapshot.
    desenhar(snapshot);
    entrarNaAutenticacao(snapshot, veredito, $("campo-servidor").value.trim());
  } catch (falha) {
    subsistemas("", "·");
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
}

/**
 * Diz até onde o link recém-criado chega, embaixo dele.
 *
 * O `alcance` é o degrau da escada do ADR 0022 em que o servidor parou, como
 * nome estável — `PortaNoRoteador`, `Ipv6Direto` ou `SoRedeLocal` —, e a frase
 * mora no `FRASES`, que é onde moram todas.
 *
 * Só os degraus que **não** alcançam de fora ganham destaque, e são os únicos
 * que precisam: os outros são boas notícias, e uma boa notícia gritada vira
 * ruído que se aprende a ignorar — inclusive no dia em que a notícia for ruim.
 *
 * `RedeLocalOuVpn` conta como perto: quem hospeda com uma VPN de navegação
 * ligada tem um endereço que parece alcançar o mundo e não aceita ninguém.
 */
function mostrarAlcance(alcance, portaRecusada) {
  const onde = $("convite-alcance");
  const frase = fraseDeErro(alcance);
  const soPerto = alcance === "SoRedeLocal" || alcance === "RedeLocalOuVpn";

  onde.textContent = frase;
  onde.classList.toggle("convite-alcance-curto", soPerto);
  onde.classList.toggle("convite-alcance-longe", !soPerto);

  // O que o roteador respondeu, quando ele respondeu alguma coisa. Vai embaixo
  // e menor: é a pista de quem for investigar, não a mensagem de quem só quer
  // mandar o link.
  if (portaRecusada) {
    const detalhe = document.createElement("span");
    detalhe.className = "convite-alcance-detalhe";
    detalhe.textContent = `o roteador respondeu: ${portaRecusada}`;
    onde.append(detalhe);
  }

  onde.hidden = false;
}

/**
 * Vira anfitrião: sobe o Dogma dentro deste app e entra nele.
 *
 * Duas etapas de propósito. `hospedar` põe o servidor de pé e devolve o link;
 * conectar é o caminho de sempre, com o endereço que ele devolveu. Um Dogma
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
    mostrarAlcance(anfitriao.alcance, anfitriao.porta_recusada);
    $("convite").hidden = false;
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
