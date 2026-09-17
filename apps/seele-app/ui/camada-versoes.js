// Versões lado a lado — ADR 0046.
//
// # O que estava faltando
//
// O núcleo (`seele-lancador`) estava pronto, com noventa testes, e **nenhum
// crate dependia dele**. Depois de ligado, ele listava e abria versões
// instaladas — e nada nunca instalava uma, então a lista era sempre vazia e o
// bloco ficava sempre escondido. Um caminho alcançável sobre uma fonte que
// nunca tinha nada é a mesma coisa que caminho nenhum.
//
// Este arquivo fecha as duas pontas que faltavam:
//
// - **baixar uma versão do catálogo**, que é o que faz a lista deixar de ser
//   vazia;
// - **abrir a versão do servidor ao entrar nele**, que é a frase da decisão:
//   «ao conectar, o cliente roda a versão daquele servidor».
//
// # Por que o link, e não o fio
//
// A versão que hospeda viaja no `seele://`, e é lida **antes** de conectar. Um
// servidor de uma versão anterior pode falar um protocolo que este cliente já
// não alcança: ele é recusado com «versão incompatível» sem chegar a dizer uma
// palavra sobre si. O link chega antes disso, por uma conversa, e não depende
// de protocolo nenhum.

"use strict";

/**
 * Abre a versão que hospeda aquele servidor, já indo para lá.
 *
 * **Esta janela não fecha**, e é a mesma razão de sempre: quem lançou não
 * segura o processo filho, e fechar daqui deixaria a pessoa sem nada na tela se
 * o outro não subisse.
 */
async function abrirNaVersaoDoServidor(versao, link) {
  const erro = $("boot-erro");
  erro.hidden = true;
  try {
    await invoke("abrir_versao", {
      versao,
      hospedar: false,
      nomePublico: null,
      link,
    });
    mostrarEtapa(`abrindo a versão ${versao}, que é a deste servidor`);
  } catch (falha) {
    erro.hidden = false;
    erro.textContent = fraseDeErro(falha);
  }
}

/** Desenha a lista de versões em CONFIGURAÇÕES. */
async function desenharPainelDeVersoes() {
  let versoes = [];
  try {
    versoes = await invoke("versoes_instaladas");
  } catch (falha) {
    console.warn("versoes:", falha);
  }

  const lista = $("lista-versoes-instaladas");
  if (versoes.length === 0) {
    // **Vazia não é defeito**, e a frase precisa dizer isso: é o estado de toda
    // instalação que nunca baixou uma segunda versão, que é quase todas.
    repovoar(lista, [
      elemento(
        "li",
        "server-dispositivos-vazio",
        "NENHUMA VERSÃO GUARDADA AO LADO DESTA",
      ),
    ]);
    return;
  }
  repovoar(
    lista,
    versoes.map((v) => {
      const linha = elemento("li");
      const caixa = elemento("div", "server-dispositivo mods-linha-gestao");
      const texto = elemento("span", "server-dispositivo-nome");
      texto.append(elemento("span", "mods-id", v.versao));
      if (v.em_uso) {
        texto.append(elemento("span", "mods-versao", "é a que está aberta agora"));
      } else if (!v.abrivel) {
        // Dita, e não oferecida: a instalação está lá e não sabe dizer o que
        // rodar — marcador antigo, ou executável apagado depois.
        texto.append(
          elemento("span", "mods-recusado", "sem procedência: reinstale para usar"),
        );
      }
      caixa.append(texto);
      if (!v.em_uso && v.abrivel) {
        const botao = elemento("button", "botao-fantasma");
        botao.type = "button";
        botao.textContent = "ABRIR";
        botao.addEventListener("click", () => {
          invoke("abrir_versao", {
            versao: v.versao,
            hospedar: false,
            nomePublico: null,
            link: null,
          }).catch((falha) => console.warn("abrir versão:", falha));
        });
        caixa.append(botao);
      }
      linha.append(caixa);
      return linha;
    }),
  );
}

/** Baixa do catálogo a versão mais nova e a guarda ao lado desta. */
async function baixarAMaisNova() {
  const botao = $("versoes-baixar");
  const estado = $("versoes-estado");
  botao.disabled = true;
  estado.textContent = "procurando no catálogo…";
  estado.classList.remove("mods-recusado");
  try {
    const versao = await invoke("baixar_versao");
    estado.textContent = `${versao} guardada ao lado desta. Ela não substituiu nada — a versão aberta continua sendo a mesma.`;
  } catch (falha) {
    // `JaInstalada` não é falha e não merece vermelho: é a resposta certa para
    // quem apertou duas vezes.
    const jaTem = falha && typeof falha === "object" && "JaInstalada" in falha;
    estado.textContent = jaTem
      ? `${falha.JaInstalada} já está guardada aqui.`
      : fraseDeErro(falha);
    if (!jaTem) estado.classList.add("mods-recusado");
  } finally {
    botao.disabled = false;
  }
  await desenharPainelDeVersoes();
  // O bloco da tela de entrada lê a mesma lista, e ela acabou de mudar.
  desenharVersoes().catch((falha) => console.warn("versões da entrada:", falha));
}

$("versoes-baixar").addEventListener("click", () => {
  baixarAMaisNova().catch((falha) => console.warn("baixar versão:", falha));
});
