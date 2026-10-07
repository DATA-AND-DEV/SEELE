// A sessão pintada com a mesma massa que a comp desenha, para a foto poder ser
// posta ao lado dela. O diálogo da porta fecha porque ele cobre a tela — ele
// tem roteiro próprio se for o assunto.
//
// **Reprova se a sessão não abrir.** Este roteiro relatou `sessao=escondida`
// com a carga limpa enquanto o duble respondia `null` a `servidores_guardados`,
// e a foto saía da tela de entrada sem ninguém dizer nada.
await entrarNaSessao();
relatar(telas("sessao"));
relatar("porta: " + visivel("porta"));
exigir(visivel("tela-boot") === "escondida", telas("a entrada continuou na frente da sessão"));
