## O que baixar

**Um arquivo por sistema.** Ele traz as duas metades do SEELE: o cliente
gráfico e as duas ferramentas de terminal.

| sistema | baixe |
|---|---|
| **Windows** | `SEELE_<versão>_x64-instalador.exe` |
| **macOS (Apple Silicon)** | `SEELE_<versão>_aarch64.dmg` |

**Linux e Mac Intel ainda não têm pacote**, e esta linha existe para dizer isso
em vez de deixar descobrir. Quem procurava um `.deb` ou um `.dmg` universal
nesta página estava procurando um arquivo que nunca foi construído — a tabela
os prometia desde a v0.11.0.

Quem está nesses dois sistemas compila do código, e o README diz como.

Dentro de cada um vão três programas: **`SEELE`**, o cliente gráfico, que tem um
botão **HOSPEDAR AQUI** e com o qual você nunca precisa abrir um terminal;
**`connection`**, o cliente de terminal; e **`seeled`**, o servidor, para quem quer um
Server no ar o tempo todo — só uma das máquinas precisa dele.

Os outros arquivos desta página não são para instalar. O `SHA256SUMS` serve para
conferir que o download chegou inteiro; os `.sig` e o `latest.json` são como o
próprio SEELE se atualiza sozinho, e os `.app.tar.gz` são o que ele baixa
quando faz isso.

**Onde ficam o `connection` e o `seeled`, como conferir a assinatura, o que fazer se o
sistema reclamar ao abrir, e como testar em cinco minutos** estão no
[README do projeto](https://github.com/DATA-AND-DEV/SEELE#readme). Esta página
fica com o que muda a cada versão.
