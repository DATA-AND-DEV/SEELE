// SEELE · Entry Plug — a tela de entrada (`#tela-boot`).
//
// Onde você já esteve, o convite colado, e a inserção do plug — mais hospedar
// aqui dentro, que é a mesma inserção com um Dogma que este processo acabou de
// subir. Sai daqui para `#tela-sessao`, e é `tela-sessao.js` que desenha o que
// chega do `connect`.

"use strict";

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
  for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "…";

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

    for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "ok";

    $("tela-boot").hidden = true;
    $("tela-sessao").hidden = false;
    mostrarVeredito(veredito);
    desenhar(snapshot);

    // Entrar no primeiro Cage e abrir a primeira Linha é o que um cliente
    // acabado de conectar deve fazer — chegar numa tela vazia é chegar sem
    // saber o que fazer.
    if (snapshot.cages.length > 0) await invoke("insert_plug", { cage: snapshot.cages[0].id });
    if (snapshot.lines.length > 0) await invoke("open_line", { line: snapshot.lines[0].id });
    await atualizar();
  } catch (falha) {
    for (const id of ["sub-melchior", "sub-balthasar", "sub-casper"]) $(id).textContent = "·";
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  } finally {
    botao.disabled = false;
  }
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
