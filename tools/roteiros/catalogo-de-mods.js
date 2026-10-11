// O catálogo de MODs, apertado de verdade — ADR 0045.
//
// Prova o que nenhum guarda de texto-fonte alcança: que BUSCAR desenha o que o
// catálogo devolveu, que INSTALAR pede **aquele** MOD naquela versão, e que um
// catálogo cuja assinatura não confere vira uma frase que manda parar — e não
// uma lista vazia, que é como uma recusa de segurança some de vista.
//
// **E reprova quando não prova** (lote W3 da F5). Até então só a seção abrir era
// `exigir`; o resto era `relatar`, e uma linha a menos, o selo trocado ou a
// recusa de assinatura calada saíam com a carga limpa. As reversões de cada
// exigência estão no relatório do lote W3
// (`docs/superpowers/sdd/2026-10-08-0.16/F5/lote-w3-report.md`).

// `api` é a que este build aceita, lida do Rust pelo aparelho: um número fixo
// aqui envelheceu (era 1) e o build de verdade teria escondido as duas versões.
const CATALOGO = {
  esquema: 1,
  gerado_em: 1757100000,
  mods: [
    {
      id: "alguem/rpg",
      titulo: "Salas de RPG",
      resumo: "Fichas, dados e salas por mesa.",
      repo: "https://example.invalid/rpg",
      // **O selo do MOD é diferente do da versão exibida, de propósito**: o `nivel`
      // do topo é a avaliação mais recente e serve para listar, e quem carimba um
      // número de versão é o `nivel` de dentro dela. Com os dois iguais, uma linha
      // que lesse o selo errado desenharia o mesmo texto e passaria.
      nivel: "oficial",
      versoes: [
        {
          versao: "1.0.0",
          api: SEELE_APIS_ACEITAS[0],
          hash: "9f2c",
          nivel: "com-notas",
          notas: ["fala-com-terceiro"],
          alcanca: ["falar com um serviço de fora"],
          arquivos: ["mod.json", "cliente/main.js"],
        },
        {
          versao: "1.1.0",
          api: SEELE_APIS_ACEITAS[0],
          hash: "4b71",
          nivel: "verificado",
          notas: [],
          // **Cada versão declara o seu alcance, e os dois são diferentes**: a
          // linha mostra o da versão exibida, e uma que mostrasse o da antiga
          // (ou nenhum) só reprova se os dois dizem coisas diferentes.
          alcanca: ["mensagens do servidor"],
          arquivos: ["mod.json", "cliente/main.js"],
        },
      ],
    },
  ],
};

SEELE_RESPOSTAS.catalogo_de_mods = CATALOGO;
SEELE_RESPOSTAS.mods_instalados = [];
SEELE_RESPOSTAS.estou_hospedando = false;

// Abrir CONFIGURAÇÕES e a seção MODS. **De dentro de uma sessão**: sem ela a
// seção nem aparece (`atualizarServer` a esconde), e abrir pela entrada fazia
// o clique cair numa seção escondida e o roteiro estourar três passos adiante.
await entrarNaSessao();
document.getElementById("botao-server-sessao").click();
await espera(150);
document.getElementById("secao-mods").click();
await espera(200);

relatar(`painel MODS: ${visivel("painel-mods")}`);
exigir(visivel("painel-mods") === "VISIVEL", "a seção MODS não abriu");
// Nada foi buscado ao abrir — ADR 0026.
const buscouAoAbrir = window.__SEELE_CHAMADAS.includes("catalogo_de_mods");
relatar(`buscou ao abrir: ${buscouAoAbrir}`);
exigir(!buscouAoAbrir,
  "abrir a seção MODS foi à rede buscar o catálogo sem ninguém apertar BUSCAR (ADR 0026)");

document.getElementById("catalogo-buscar").click();
await espera(300);
const MOD = CATALOGO.mods[0];
const linhas = [...document.querySelectorAll("#lista-catalogo li")];
relatar(`linhas no catálogo: ${linhas.length}`);
exigir(linhas.length === CATALOGO.mods.length,
  `BUSCAR não desenhou o que o catálogo devolveu: ${linhas.length} linhas desenhadas, ` +
    `e o catálogo trouxe ${CATALOGO.mods.length}`);
exigir(linhas[0].textContent.includes(MOD.titulo) && linhas[0].textContent.includes(MOD.id),
  `a linha desenhada não é a do MOD do catálogo («${MOD.titulo}», ${MOD.id}): «${linhas[0].textContent}»`);
const estadoDaBusca = document.getElementById("catalogo-estado").textContent;
relatar(`estado: ${estadoDaBusca}`);
exigir(estadoDaBusca === `${CATALOGO.mods.length} no catálogo.`,
  `o estado da busca diz «${estadoDaBusca}», e o catálogo trouxe ${CATALOGO.mods.length}`);
// O selo tem de ser o da versão exibida — a 1.1.0, verificada — e não o do MOD,
// que no catálogo deste roteiro é outro.
const ultima = MOD.versoes[MOD.versoes.length - 1];
const seloEsperado = `versão ${ultima.versao} · VERIFICADO`;
const selo = [...document.querySelectorAll("#lista-catalogo .mods-versao")]
  .map((el) => el.textContent)
  .find((texto) => texto.startsWith("versão "));
relatar(`selo da versão exibida: ${selo ?? "—"}`);
exigir(selo === seloEsperado,
  `o selo desenhado é «${selo ?? "nenhum"}», e devia ser «${seloEsperado}»: ` +
    `a versão exibida é a ${ultima.versao}, verificada, e o MOD é «${MOD.nivel}»`);
// E o alcance também é o da versão exibida (lote W3, rodada 1): é o que a
// pessoa lê antes de instalar. A 1.0.0 declara outro.
const alcanceDesenhado = [...linhas[0].querySelectorAll(".mods-etiqueta")].map((el) => el.textContent);
const alcanceEsperado = ultima.alcanca;
relatar(`alcance da versão exibida: ${alcanceDesenhado.join(" · ") || "nenhum"}`);
exigir(alcanceDesenhado.join(" · ") === alcanceEsperado.join(" · "),
  `o alcance desenhado é [${alcanceDesenhado.join(" · ")}], e o da versão exibida (${ultima.versao}) é ` +
    `[${alcanceEsperado.join(" · ")}]; o da ${MOD.versoes[0].versao} é [${MOD.versoes[0].alcanca.join(" · ")}]`);

window.__SEELE_CHAMADAS.length = 0;
delete window.__SEELE_ARGS.instalar_mod_do_catalogo;
document.querySelector("#lista-catalogo button").click();
await espera(300);
const pedido = window.__SEELE_ARGS.instalar_mod_do_catalogo;
relatar(`pediu instalação: id=${pedido?.id} versao=${pedido?.versao}`);
exigir(pedido !== undefined,
  `INSTALAR não pediu nada: instalar_mod_do_catalogo nunca foi chamado (a sequência foi: ${window.__SEELE_CHAMADAS.join(" → ") || "nenhuma"})`);
exigir(pedido.id === MOD.id && pedido.versao === ultima.versao,
  `INSTALAR pediu «${pedido.id} ${pedido.versao}», e a linha desenhada era a ${MOD.id} ${ultima.versao}`);

// E a recusa de assinatura: tem de virar frase, e não lista vazia.
SEELE_RESPOSTAS.catalogo_de_mods = { __recusa: "AssinaturaDoCatalogoRecusada" };
document.getElementById("catalogo-buscar").click();
await espera(300);
const estado = document.getElementById("catalogo-estado");
relatar(`recusa vira frase: ${estado.textContent.split("\n")[0]}`);
exigir(estado.textContent.startsWith("O CATÁLOGO NÃO É O NOSSO.") && estado.textContent.includes("Não tente de novo"),
  "a assinatura do catálogo que não confere não virou a frase que manda parar: " +
    `o estado diz «${estado.textContent.split("\n")[0]}»`);
relatar(`marcada como erro: ${estado.classList.contains("mods-recusado")}`);
exigir(estado.classList.contains("mods-recusado"),
  "a recusa de assinatura foi escrita como um estado qualquer, e não marcada como erro (mods-recusado)");
