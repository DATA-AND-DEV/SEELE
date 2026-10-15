// O grupo FORA DO AR (P6, D-fio-4): os membros deste servidor que não estão
// conectados, desenhados pela lista que o `membros_fora_do_ar` devolve — e não
// pelo retrato, que só diz o número da revisão (nenhum MOD da API 5 ganha a
// lista de membros).
//
// **Reprova** se o grupo não aparecer, se ele não for o último, se uma linha
// carregar mais que o nome, se a lista for pedida de novo sem a revisão andar,
// se a revisão andar e o grupo não trocar, ou se a lista de um servidor
// aparecer na sessão do seguinte.
SEELE_RESPOSTAS.membros_fora_do_ar = [
  { id: 7, nickname: "kaji" },
  { id: 8, nickname: "misato" },
];
SEELE_QUADRO.fora_do_ar_revision = 1;
await entrarNaSessao();

const grupo = () => document.querySelector("#lista-roster .roster-grupo[data-fora-do-ar='sim']");
const nomes = () => [...(grupo()?.querySelectorAll(".membro-fora-do-ar") ?? [])].map((l) => l.textContent);
const pedidos = () => window.__SEELE_CHAMADAS.filter((c) => c === "membros_fora_do_ar").length;

exigir(await ate(() => grupo() !== null, 1500), "o grupo FORA DO AR não apareceu com dois membros fora do ar");
relatar("revisão 1: " + grupo().querySelector(".roster-sala").textContent + " · " + nomes().join(", "));
exigir(grupo().querySelector(".roster-sala-nome").textContent === "FORA DO AR",
  "o grupo se chama «" + grupo().querySelector(".roster-sala-nome").textContent + "»");
exigir(grupo().querySelector(".roster-sala-conta").textContent === "2",
  "a conta do grupo diz «" + grupo().querySelector(".roster-sala-conta").textContent + "», e são dois");
exigir(JSON.stringify(nomes()) === JSON.stringify(["kaji", "misato"]), "os nomes do grupo são " + nomes());
exigir(document.querySelector("#lista-roster > .roster-grupo:last-of-type") === grupo(),
  "o FORA DO AR não é o último grupo da lista de pessoas");

// **Só o nome.** Sem horário, sem sinal, sem botão: a linha é o apelido.
for (const linha of grupo().querySelectorAll(".membro-fora-do-ar")) {
  exigir(linha.children.length === 0, "uma linha do FORA DO AR carrega mais que o nome: " + linha.outerHTML);
}
// E não é a linha de quem está aqui, que MODs apresentam e o celular abre.
exigir(grupo().querySelector("li.pessoa") === null, "uma linha do FORA DO AR virou `li.pessoa`");

// **Pela revisão.** Quadros com o mesmo número não pedem a lista de novo, nem
// o `RosterChanged` que cada pessoa manda uma vez por segundo (pendência 63): o
// desenho do tique é a cada 500 ms, e cada evento pede outro.
const antes = pedidos();
for (let i = 0; i < 3; i++) {
  window.__SEELE_EMITIR("RosterChanged");
  await espera(100);
}
await espera(500);
exigir(pedidos() === antes, `a lista foi pedida ${pedidos() - antes} vez(es) sem a revisão andar`);

// A revisão anda: um dos dois entrou no servidor.
SEELE_RESPOSTAS.membros_fora_do_ar = [{ id: 8, nickname: "misato" }];
SEELE_QUADRO.fora_do_ar_revision = 2;
// O número sobe antes do aviso, como no `fold` da ponte.
window.__SEELE_EMITIR("RosterChanged");
exigir(await ate(() => nomes().length === 1, 1500), "a revisão andou e o grupo continuou com " + nomes());
relatar("revisão 2: " + nomes().join(", "));

// Ninguém fora do ar: o grupo some, em vez de ficar com conta zero.
SEELE_RESPOSTAS.membros_fora_do_ar = [];
SEELE_QUADRO.fora_do_ar_revision = 3;
window.__SEELE_EMITIR("RosterChanged");
exigir(await ate(() => grupo() === null, 1500), "com ninguém fora do ar, o grupo ficou: " + nomes());
relatar("revisão 3: sem grupo");

// **A lista é de um servidor.** Sai-se com um fora do ar, e o seguinte
// começa com a mesma revisão e ninguém fora do ar: guardar a revisão do
// anterior faria a primeira busca concluir que nada mudou.
SEELE_RESPOSTAS.membros_fora_do_ar = [{ id: 7, nickname: "kaji" }];
SEELE_QUADRO.fora_do_ar_revision = 4;
window.__SEELE_EMITIR("RosterChanged");
exigir(await ate(() => nomes().length === 1, 1500), "a revisão 4 não desenhou o kaji");
await ejetar();
SEELE_RESPOSTAS.membros_fora_do_ar = [];
await entrarNaSessao();
exigir(pedidos() > 0 && await ate(() => document.querySelector("#lista-roster .roster-grupo") !== null, 600),
  "a lista de pessoas do servidor seguinte não se desenhou");
exigir(grupo() === null, "o FORA DO AR do servidor anterior apareceu no seguinte: " + nomes());
relatar("depois de trocar de servidor: sem grupo");
