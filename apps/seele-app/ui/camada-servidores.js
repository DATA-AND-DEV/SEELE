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
      // **O servidor roda outra versão, e ela está aqui** — ADR 0046.
      //
      // Perguntado antes de conectar de propósito: um servidor de uma versão
      // anterior pode falar um protocolo que este cliente já não alcança, e
      // seria recusado com «versão incompatível» sem chegar a dizer uma palavra
      // sobre si. O link chega antes disso.
      //
      // Só quando a versão **está instalada**: oferecer abrir o que não está
      // no disco seria um botão que falha. Baixá-la é o outro caminho, e o
      // botão dele fica em CONFIGURAÇÕES · VERSÕES.
      if (convite.pode_abrir_naquela_versao) {
        fecharServidores();
        abrirNaVersaoDoServidor(convite.versao, escrito);
        return;
      }
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
  // O apelido daquela visita quando se sabe; senão nenhum, e `conectar` lê o
  // desta máquina.
  //
  // Esta leitura morava aqui, e era a **única** porta que a fazia: quem
  // hospedava não passava por ela e conectava sem nome. Ela desceu para o
  // `conectar`, que é por onde toda conexão passa.
  conectar(alvo, apelido || undefined, token).catch((falha) =>
    console.warn("conectar:", falha),
  );
}

// --------------------------------------------------------------------------
// O caminho entre pares — §5 da spec de tela
// --------------------------------------------------------------------------
//
// **O opt-in que faltava.** O núcleo, o protocolo e o servidor traziam a malha
// inteira desde a v0.11.0, e nada nesta casca declarava o consentimento — o
// `enlace.rs` registrava isso por escrito («nada em `apps/` nem no `seele-ffi`
// chama...») e o roteiro de duas máquinas marcava o passo como bloqueado. Sem
// um lugar para clicar, `emprestando` era sempre falso em produção e a tela
// vinha sempre do servidor.
//
// São dois interruptores, e o `control.rs` é explícito em não juntá-los: eles
// respondem perguntas diferentes, e quem aceita uma não aceita a outra por
// tabela.


/** O que está guardado agora, para os dois botões lerem sem perguntar de novo. */
let malhaEmMaos = { pares_que_atende: 0, assiste_por_par: false, teto_desta_versao: 1 };

/** Lê o que está guardado e desenha as duas escolhas. */
async function desenharMalha() {
  try {
    malhaEmMaos = await invoke("caminho_entre_pares");
  } catch (falha) {
    console.warn("malha:", falha);
    return;
  }
  const empresta = malhaEmMaos.pares_que_atende > 0;
  const assiste = malhaEmMaos.assiste_por_par;

  // **Estado escrito por extenso, e não um botão que muda de cor.** É a regra
  // desta tela, e o comentário de APARÊNCIA a diz inteira: uma linha que
  // informa vale mais que um controle que promete.
  $("malha-assiste-estado").textContent = assiste
    ? "aceito: a tela pode vir de outra pessoa"
    : "não aceito: a tela vem sempre do servidor";
  $("malha-empresta-estado").textContent = empresta
    ? "aceito: posso servir a tela a mais alguém"
    : "não empresto a minha subida";
  $("malha-assiste").textContent = assiste ? "DESLIGAR" : "LIGAR";
  $("malha-empresta").textContent = empresta ? "DESLIGAR" : "LIGAR";

  // O teto vem do núcleo, e não escrito aqui: duas cópias do mesmo número
  // divergem no dia em que ele subir, e a tela passaria a prometer o que o
  // cliente não cumpre.
  $("malha-teto").textContent =
    malhaEmMaos.teto_desta_versao === 1
      ? "Esta versão serve no máximo uma pessoa por vez."
      : `Esta versão serve no máximo ${malhaEmMaos.teto_desta_versao} pessoas ao mesmo tempo.`;
}

/** Guarda a escolha, e a declara na sessão se houver uma. */
async function guardarMalha(empresta, assiste) {
  const estado = $("malha-estado");
  try {
    await invoke("consentir_no_caminho_entre_pares", {
      paresQueAtende: empresta ? malhaEmMaos.teto_desta_versao : 0,
      assistePorPar: assiste,
    });
    // **Dito, e não suposto.** A retirada alcança o que já está no ar: o núcleo
    // derruba os caminhos de par abertos antes de a declaração nova sair, e
    // quem estava sendo servido volta a ser servido pelo servidor. Quem acabou
    // de desligar precisa saber que isso já aconteceu.
    estado.textContent =
      empresta || assiste
        ? "Guardado. Vale nesta sessão e nas próximas."
        : "Guardado: você não participa da malha. A tela vem do servidor.";
  } catch (falha) {
    estado.textContent = fraseDeErro(falha);
  }
  await desenharMalha();
}

$("malha-assiste").addEventListener("click", () => {
  guardarMalha(malhaEmMaos.pares_que_atende > 0, !malhaEmMaos.assiste_por_par).catch((falha) =>
    console.warn("malha:", falha),
  );
});

$("malha-empresta").addEventListener("click", () => {
  guardarMalha(malhaEmMaos.pares_que_atende === 0, malhaEmMaos.assiste_por_par).catch((falha) =>
    console.warn("malha:", falha),
  );
});
