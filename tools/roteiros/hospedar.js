// HOSPEDAR AQUI entra direto na sessão, sem passar pela conferência de chave.
//
// A `#tela-auth` existe para o momento TOFU do ADR 0003 — olhar a impressão
// digital de um servidor alheio antes de entrar. Hospedando não há chave alheia,
// e o pedágio que não decide nada ensina a atravessar sem ler.
// Este roteiro mede **caminho**, e não desenho: o pintor da sessão fica de fora
// para uma falha de pintura não ser lida como falha de navegação.
desenhar = () => {};
atualizar = async () => {};
relatar(telas("antes"));
document.getElementById("botao-hospedar").click();
await espera(700);
relatar(telas("depois de HOSPEDAR"));
const registro = document.getElementById("auth-registro");
relatar("registro: " + registro.textContent.replace(/\s+/g, " ").slice(0, 160));
