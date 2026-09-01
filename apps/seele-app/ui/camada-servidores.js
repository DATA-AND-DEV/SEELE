// Onde você já esteve: os servidores conhecidos, e um campo para outro.
//
// # O que esta camada muda de comportamento
//
// A comp da 0.9.0 tira o endereço da tela de entrada e o traz para aqui. O
// ganho é o caso comum: quem já usou o produto volta ao mesmo lugar, e voltar
// deixa de ser digitar um endereço para virar apertar uma linha.
//
// O campo continua embaixo porque o caso raro não pode sumir — e ele aceita
// tanto o endereço cru quanto um `seele://`, que é a forma em que alguém manda
// um servidor para outra pessoa.
//
// # O que ela não decide
//
// Como se conecta. Ela chama o que a tela de entrada já chamava; a diferença é
// só de onde vem o endereço.

"use strict";

/** Para onde o teclado volta quando esta camada fecha. */
let focoAntesDosServidores = null;

/**
 * Quando foi a última visita, em palavra.
 *
 * Relativo e não a data: «há 3 dias» responde a pergunta que se faz olhando
 * esta lista — *qual destes eu usei por último* —, e uma data obriga quem lê a
 * fazer a subtração de cabeça.
 */
function quandoFoi(segundos) {
  if (!segundos) return "";
  const agora = Math.floor(Date.now() / 1000);
  const passou = Math.max(0, agora - segundos);
  if (passou < 60) return "agora há pouco";
  if (passou < 3600) return `há ${Math.floor(passou / 60)} min`;
  if (passou < 86_400) return `há ${Math.floor(passou / 3600)} h`;
  return `há ${Math.floor(passou / 86_400)} d`;
}

/** Desenha a lista do que está gravado em disco. */
async function desenharServidores() {
  let conhecidos = [];
  try {
    conhecidos = await invoke("conhecidos");
  } catch (falha) {
    console.warn("conhecidos:", falha);
  }

  const lista = $("servidores-lista");
  // Vazio **e dizendo por quê**: uma lista sem linhas e sem explicação lê como
  // defeito, e este é o estado de quem abriu o produto pela primeira vez.
  $("servidores-vazio").hidden = conhecidos.length > 0;

  lista.replaceChildren(
    ...conhecidos.map((conhecido) => {
      const item = elemento("li");
      const botao = elemento("button", "servidor-linha");
      botao.type = "button";
      botao.dataset.alvo = conhecido.alvo;
      botao.dataset.apelido = conhecido.apelido ?? "";
      // O nome, para a caixa de confirmação da troca falar por nome.
      if (conhecido.nome) botao.dataset.nome = conhecido.nome;

      const nome = conhecido.nome || conhecido.alvo;
      // **O ladrilho traz a imagem do servidor quando há uma.**
      //
      // Ela é gravada desde que `lembrar_aparencia_do_servidor` existe, e esta
      // lista nunca a leu: desenhava sempre a sigla, e a sigla vinha do
      // **endereço** mesmo quando o nome estava guardado. Duas coisas erradas na
      // mesma caixa de 34px — «lista de server não trazem o último nome salvo
      // nem o ícone».
      //
      // A sigla continua sendo o que aparece sem imagem, e passa a sair do nome
      // quando ele existe: `CA` de «Casa» é o que alguém reconhece, e `19` de
      // `192.168.0.39` não é nada.
      const ladrilho = elemento("span", "servidor-sigla", siglaDoAlvo(nome));
      if (conhecido.icone) {
        ladrilho.style.backgroundImage = `url(${uriDeIcone(conhecido.icone)})`;
        ladrilho.textContent = "";
        ladrilho.dataset.comImagem = "sim";
      }
      botao.append(
        ladrilho,
        (() => {
          const texto = elemento("span", "servidor-texto");
          texto.append(
            elemento("span", "servidor-nome", nome),
            elemento("span", "servidor-endereco", conhecido.alvo),
          );
          return texto;
        })(),
        elemento("span", "servidor-visto", quandoFoi(conhecido.visto_em)),
      );
      botao.title = `entrar em ${nome} como ${conhecido.apelido || "você mesmo"}`;
      item.append(botao);

      // O esquecer, **fora** do botão que entra: um `<button>` dentro de outro
      // não é marcação válida, e o alvo de cada gesto tem de ser só o seu.
      const esquecer = elemento("button", "servidor-esquecer", "×");
      esquecer.type = "button";
      esquecer.dataset.esquecer = conhecido.alvo;
      esquecer.title = `esquecer ${nome}`;
      esquecer.setAttribute("aria-label", `Esquecer ${nome}`);
      item.append(esquecer);
      return item;
    }),
  );
}

/** Abre a camada. */
async function abrirServidores() {
  focoAntesDosServidores = document.activeElement;
  $("servidores-erro").hidden = true;
  $("servidores-endereco").value = "";
  await desenharServidores();
  $("servidores").hidden = false;
  $("servidores-endereco").focus();
  anunciar("Onde você já esteve. Escape fecha.");
}

/** Fecha, devolvendo o teclado. */
function fecharServidores() {
  $("servidores").hidden = true;
  if (focavel(focoAntesDosServidores)) focoAntesDosServidores.focus();
  focoAntesDosServidores = null;
}

/** Diz, onde se acabou de digitar, por que não deu. */
function recusarServidor(frase) {
  const onde = $("servidores-erro");
  onde.hidden = false;
  onde.textContent = frase;
}

// Uma linha da lista: entra naquele servidor, com o apelido daquela vez.
$("servidores-lista").addEventListener("click", async (evento) => {
  // O esquecer primeiro: ele está dentro do mesmo `<li>` que a linha, e
  // procurar a linha antes o pegaria como se fosse ela.
  const esquecer = evento.target.closest("button[data-esquecer]");
  if (esquecer) {
    try {
      await invoke("esquecer", { alvo: esquecer.dataset.esquecer });
      anunciar("Servidor esquecido.");
    } catch (falha) {
      console.warn("esquecer:", falha);
    }
    await desenharServidores();
    return;
  }
  const linha = evento.target.closest("button[data-alvo]");
  if (!linha) return;
  fecharServidores();
  irParaOServidor(linha.dataset.alvo, linha.dataset.apelido, null, linha.dataset.nome);
});

// O campo: um endereço cru ou um `seele://`.
//
// O link é resolvido **antes** de conectar, e não depois: ele carrega a
// impressão digital do certificado, e é ela que faz quem recebe não precisar
// conferi-la por outro canal. Tratá-lo como endereço perderia essa metade.
$("servidores-forma").addEventListener("submit", async (evento) => {
  evento.preventDefault();
  const escrito = $("servidores-endereco").value.trim();
  if (escrito === "") return;
  $("servidores-erro").hidden = true;

  let alvo = escrito;
  let token = null;
  if (escrito.startsWith("seele://")) {
    try {
      const convite = await invoke("analisar_convite", { link: escrito });
      alvo = convite.alvo;
      // **O token vem junto, e é a metade que faz o link valer.** Um
      // `seele://` com convite de uso único é uma credencial; ler só o
      // endereço dele é chegar sem ela, e o servidor recusa na porta.
      token = convite.token ?? null;
    } catch (falha) {
      console.warn("analisar_convite:", falha);
      recusarServidor(fraseDeErro(falha));
      return;
    }
  }
  fecharServidores();
  irParaOServidor(alvo, null, token);
});

$("servidores-fechar").addEventListener("click", fecharServidores);
fecharAoClicarFora("servidores", fecharServidores);

window.addEventListener(
  "keydown",
  (evento) => {
    if (evento.key !== "Escape" || $("servidores").hidden) return;
    evento.preventDefault();
    evento.stopPropagation();
    fecharServidores();
  },
  true,
);

/**
 * Vai para um servidor, venha-se de onde vier.
 *
 * Duas origens, e elas pedem coisas diferentes:
 *
 * - **da tela de entrada**, não há sessão a largar: preenche os campos que o
 *   `conectar` lê e conecta;
 * - **de dentro de uma sessão**, há: o `trocarDeServidor` pergunta antes, com a
 *   consequência escrita, porque sair de um servidor é largar a sala e a
 *   conversa. Perguntar é a regra desta janela para o que não se desfaz.
 *
 * O apelido vem junto quando se sabe — é o daquela visita, gravado com o
 * servidor. Sem ele, o campo da entrada decide.
 */
function irParaOServidor(alvo, apelido, token, nome) {
  const naSessao = !$("tela-sessao").hidden;
  if (naSessao) {
    // O nome junto: a caixa de confirmação fala por nome, e quem clicou numa
    // linha desta lista acabou de ler o nome nela.
    pedirTrocaDeServidor(alvo, apelido || undefined, nome);
    return;
  }
  // O apelido daquela visita quando se sabe; senão o desta máquina, que é o
  // que o perfil grava antes de haver servidor.
  Promise.resolve(apelido || invoke("apelido_local"))
    .then((nome) => conectar(alvo, (nome || "").trim() || "pessoa", token))
    .catch((falha) => console.warn("conectar:", falha));
}
