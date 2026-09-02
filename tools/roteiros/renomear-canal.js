// A barra do canal aberto troca de elemento conforme a permissão.
//
// O nome vira `<button>` para quem administra e volta a ser `<span>` para quem
// não. A troca é um `replaceWith` que reaproveita o `id`, e é o tipo de coisa
// que os guardas de texto não alcançam: eles leem o arquivo, não o DOM.
//
// O que se prova aqui: que o elemento troca nos dois sentidos, que o `id`
// sobrevive à troca — sem ele o `$("linha-nome")` do desenho seguinte devolve
// `null` e a barra some —, e que o nó **não** é recriado quando nada mudou, que
// é o que deixaria o teclado perder o foco duas vezes por segundo.

const canal = { id: 7, name: "geral", open: true };

const semPermissao = { channels: [canal], may_manage_voice_rooms: false };
const comPermissao = { channels: [canal], may_manage_voice_rooms: true };

desenharLinha(semPermissao);
relatar("sem permissão: <" + $("linha-nome").tagName + "> «" + $("linha-nome").textContent + "»");

desenharLinha(comPermissao);
const virouBotao = $("linha-nome");
relatar("com permissão: <" + virouBotao.tagName + "> título «" + virouBotao.title + "»");

// Duas vezes seguidas com a mesma permissão: tem de ser o mesmo nó.
desenharLinha(comPermissao);
relatar("redesenhou sem mudar nada: mesmo nó = " + ($("linha-nome") === virouBotao));

desenharLinha(semPermissao);
relatar(
  "permissão retirada: <" +
    $("linha-nome").tagName +
    "> título «" +
    $("linha-nome").title +
    "»",
);

// Sem canal aberto ninguém renomeia coisa nenhuma.
desenharLinha({ channels: [], may_manage_voice_rooms: true });
relatar("sem canal aberto: <" + $("linha-nome").tagName + "> «" + $("linha-nome").textContent + "»");
