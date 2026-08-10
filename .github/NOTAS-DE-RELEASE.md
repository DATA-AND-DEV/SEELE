## O que baixar

| você quer | baixe |
|---|---|
| **Windows** — cliente gráfico | `SEELE_<versão>_x64-setup.exe` ou `SEELE_<versão>_x64_en-US.msi` |
| **Windows** — terminal e servidor | `seele-cli-<versão>-windows-x86_64.zip` |
| **macOS** — cliente gráfico | `SEELE_<versão>_universal.dmg` (Intel e Apple Silicon) |
| **macOS** — terminal e servidor | `seele-cli-<versão>-macos.tar.gz` (universal) |
| **Linux** — cliente gráfico | `.deb` (Debian/Ubuntu) ou `.AppImage` (qualquer distro) |
| **Linux** — terminal e servidor | `seele-cli-<versão>-linux.tar.gz` |

Os arquivos `seele-cli-*` contêm dois programas:

- **`plug`** — o cliente de terminal, que é o produto principal.
- **`seeled`** — o servidor. Só uma das máquinas precisa rodá-lo.

Nenhum dos dois precisa de instalação: descompacte e execute.

---

## Nada aqui é assinado

**Este é o aviso mais importante desta página.** Os binários não têm assinatura
de código, porque assinar exige certificado pago da Apple e de uma autoridade
Windows. O sistema operacional vai reclamar, e a reclamação não significa que o
arquivo esteja corrompido ou infectado — significa que ninguém pagou para
garantir quem o produziu.

**macOS.** O Gatekeeper vai dizer que o app "está danificado e não pode ser
aberto". Ele não está. Depois de arrastar para Aplicativos:

```sh
xattr -dr com.apple.quarantine /Applications/SEELE.app
```

Para os binários de linha de comando, o mesmo na pasta onde você os
descompactou:

```sh
xattr -dr com.apple.quarantine ./plug ./seeled
chmod +x ./plug ./seeled
```

**Windows.** O SmartScreen vai mostrar "O Windows protegeu o computador".
Clique em **Mais informações** → **Executar assim mesmo**.

**Linux.** Nada reclama. Para o AppImage, `chmod +x` antes de executar.

Se isso te incomoda — e é razoável que incomode —, a alternativa é compilar do
código-fonte, que é a coisa que este produto foi feito para permitir. Veja
`docs/windows.md` no repositório.

---

## Como testar rápido

Numa máquina:

```sh
./seeled 0.0.0.0:8383
```

Ele imprime o endereço a usar na outra máquina e a impressão digital do
certificado. **Anote a impressão digital.**

Na outra:

```sh
./plug --server <endereço>:8383 --nick seunome
```

Na primeira conexão o cliente mostra `PRIMEIRO CONTATO — CHAVE FIXADA` com uma
impressão digital. Confira contra a que o servidor imprimiu: se conferir, é
aquele servidor mesmo.

Dentro do cliente, aperte `?`.

Duas coisas que economizam tempo:

- A porta 8383 é **UDP**, não TCP. É o erro de firewall mais comum aqui, porque
  a regra que se escreve de cabeça é sempre TCP.
- **Use fones dos dois lados.** Não há cancelamento de eco, e sem fones haverá
  realimentação.

---

## O que ainda não foi validado

Este projeto nunca foi usado por duas pessoas reais em duas máquinas reais. O
que está testado é o que roda sem hardware: 457 testes automáticos, incluindo
áudio simulado sob perda de pacote e um soak de dez minutos em tempo simulado.

Não testado: voz por microfone de verdade, latência boca-a-ouvido, e o
comportamento em Windows e Linux além do que a integração contínua compila.

`docs/teste-duas-maquinas.md` é o roteiro para produzir essas medições.
