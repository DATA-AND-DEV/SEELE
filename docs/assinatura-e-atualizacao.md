# Assinatura e atualização — o passo a passo

Este documento é para uma pessoa, uma vez. Nada aqui se faz duas vezes, e é por
isso que está escrito: quem fizer isso não vai lembrar dos detalhes daqui a seis
meses, e quem herdar o projeto nunca os soube.

**Tudo o que está aqui é opcional.** Enquanto nada disto for feito, o repositório
compila, o `release.yml` roda, e a página de release sai como sempre saiu: um
instalador por sistema, para baixar à mão, com os avisos do sistema operacional
que as `NOTAS-DE-RELEASE.md` explicam. O que falta sem isto é o botão de
atualizar e o silêncio do SmartScreen.

---

## São duas assinaturas, e elas não se substituem

Confundir as duas é o erro que faz o trabalho todo parecer redundante.

| | quem confere | o que ela responde | sem ela |
|---|---|---|---|
| **do sistema operacional** | Windows e macOS, na hora de abrir | «quem produziu este arquivo?» | o SmartScreen reclama e o Smart App Control **bloqueia** |
| **do projeto** | o próprio SEELE, na hora de atualizar | «este pacote veio de vocês?» | o botão de atualizar não existe |

Um instalador com assinatura Authenticode perfeita continua sendo **qualquer**
instalador: é o `latest.json` que diz qual baixar, e é a assinatura do projeto
que impede que ele aponte para outro. As duas são necessárias e nenhuma cobre o
buraco da outra. ADR 0026.

**Faça a do projeto primeiro.** Ela é gratuita, leva cinco minutos, e é a que
resolve a queixa que custou um teste real — as duas máquinas em versões
diferentes.

### E no Windows são três defesas, não uma

Este documento dizia «SmartScreen» em todo lugar, como se fosse uma coisa só.
Não é, e a diferença decide se assinar é conforto ou se é o que torna o programa
instalável. A pergunta veio de quem estava pagando a conta, o que é a hora certa
de ela vir.

| | o que faz | assinar resolve? |
|---|---|---|
| **SmartScreen** | avisa: «O Windows protegeu o computador» | **sim**, e há contorno sem assinar — *Mais informações → Executar assim mesmo* |
| **Smart App Control** | **impede a execução**, e não oferece «executar assim mesmo» | **sim, e é a única saída** |
| **Defender Antivírus** | varre em busca de malware | **não.** Se ele acusar, é falso positivo, e o caminho é submeter a amostra à Microsoft |

O **Smart App Control** é o que mais surpreende, e foi o que apareceu no primeiro
teste com máquina de outra pessoa. Ele é do Windows 11, só liga em instalação
limpa, e passa um tempo em avaliação antes de se ligar sozinho — então duas
máquinas iguais se comportam diferente e ninguém entende por quê. Quando está
ligado não há caixa a dispensar: o programa não abre.

Uma consequência prática: **desligar o Smart App Control é caminho sem volta.**
Depois de desligado, só reinstalando o Windows para religá-lo. Não é conselho a
dar a quem só queria conversar com um amigo — o que se dá é um instalador
assinado.

E vale dizer o que a assinatura **não** compra: reputação é acumulada, então um
certificado novo pode continuar tomando aviso do SmartScreen nas primeiras
instalações, até o serviço ver o arquivo circular. O Smart App Control é o caso
em que a assinatura vale imediatamente.

---

## Parte 1 — a chave do projeto (grátis, cinco minutos)

É uma chave `minisign`. O `cargo tauri` a gera.

```sh
cargo install tauri-cli --version "^2" --locked   # se ainda não tiver
mkdir -p ~/.tauri
cargo tauri signer generate -w ~/.tauri/seele.key
```

Ele pede uma senha. **Use uma**, e guarde-a onde guarda as outras — o modo sem
senha existe para automação e não é o seu caso. No fim ficam dois arquivos:

- `~/.tauri/seele.key` — a privada. Nunca entra no repositório.
- `~/.tauri/seele.key.pub` — a pública. Vai para o repositório, em texto claro.

### 1.1 — a pública vai para o `tauri.conf.json`

Abra `~/.tauri/seele.key.pub`. É uma única linha em base64. Cole-a em
`apps/seele-app/tauri.conf.json`, no lugar da string vazia:

```json
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/DATA-AND-DEV/SEELE/releases/latest/download/latest.json"
      ],
      "pubkey": "COLE-A-LINHA-AQUI",
```

**Cuidado com o arquivo:** ele não pode ganhar BOM. Se editar no Bloco de Notas
do Windows, salve como "UTF-8" e não como "UTF-8 com BOM" — o analisador do Tauri
recusa o arquivo e a mensagem que ele dá (`expected value at line 1 column 1`)
não parece ter nada a ver.

Comite essa mudança. A chave pública é pública; é isso que ela é.

### 1.2 — a privada vira segredo do repositório

Em **Settings → Secrets and variables → Actions → New repository secret**:

| segredo | conteúdo |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | o conteúdo **inteiro** de `~/.tauri/seele.key`, copiado e colado |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | a senha que você escolheu |

O conteúdo do arquivo, e não o caminho dele: o runner não tem o seu disco.

### 1.3 — guarde a privada onde você não a perca

Escrito sem rodeio, porque é a única parte irreversível deste documento:

> **Se você perder a chave privada ou a senha, ninguém que já instalou o SEELE
> vai conseguir atualizar de novo.** Não há como rotacionar a chave para quem já
> está instalado: o app confere contra a chave que foi compilada dentro dele. A
> saída é publicar uma versão com a chave nova e pedir a cada pessoa que baixe e
> instale à mão — exatamente o problema que este trabalho existe para acabar.

Duas cópias, em lugares que não falham juntos. Um gerenciador de senhas e um
papel guardado servem.

### 1.4 — conferir

Rode o workflow **Release** pela aba Actions, com uma versão de teste
(`0.0.1`, por exemplo). Nos registros do job de cada sistema você deve ver a CLI
do Tauri dizendo `Finished 1 updater signature at:` e, no job `publicar`, o
`latest.json` impresso inteiro. Se ele não aparecer, o passo `Manifesto de
atualização` diz por quê.

O rascunho fica em Releases, não publicado. **Enquanto ele não for publicado à
mão, nenhum app do mundo o enxerga** — `releases/latest` só conta releases
publicados e que não sejam pré-lançamento. É de propósito: a decisão de lançar
continua sendo de uma pessoa.

---

## Parte 2 — assinatura do Windows (Azure Artifact Signing)

O nome mudou: o serviço se chamava **Azure Trusted Signing** e agora é **Azure
Artifact Signing**. É o mesmo produto, e é o único caminho barato para uma
assinatura que o SmartScreen aceite — na ordem de dez dólares por mês, sem token
físico, com o certificado ficando na nuvem da Microsoft.

A alternativa é um certificado OV ou EV de autoridade comum: entre 200 e 600
dólares por ano, e o EV exige um token USB que não entra num runner do GitHub.

**Conte com alguns dias**, não com uma tarde: a validação de identidade é feita
por gente.

### 2.1 — antes de começar

- Uma assinatura do Azure com cartão cadastrado.
- **Identidade validável.** Como pessoa física, a Microsoft pede três anos de
  histórico verificável (documento, endereço, presença pública). Como empresa,
  pede CNPJ e um número D-U-N-S. A validação individual costuma ser a que trava.

### 2.2 — criar a conta de assinatura

No portal do Azure:

1. **Create a resource** → procure por **Trusted Signing** (ou **Artifact
   Signing**) → **Create**.
2. Escolha um grupo de recursos, um nome para a conta e uma região. **Anote a
   região**: ela decide o endereço do serviço, que é `https://<região>.codesigning.azure.net`
   — por exemplo `https://eus.codesigning.azure.net` para East US.
3. Plano: o **Basic** basta para este volume.

### 2.3 — validar a identidade

Dentro da conta criada, **Identity validations** → **New identity validation**.
Preencha exatamente como consta nos documentos: um espaço a mais no nome da
empresa reprova. Envie e espere.

O nome que você põe aqui é o que o Windows vai mostrar como publicador do
instalador. Escolha pensando em quem vai ler o aviso.

### 2.4 — criar o perfil de certificado

Depois de a validação passar: **Certificate profiles** → **New certificate
profile** → tipo **Public Trust**. Dê um nome e anote-o.

### 2.5 — criar a identidade que o CI usa

O runner precisa entrar no Azure sozinho. Isso é um **app registration** no
Microsoft Entra ID:

1. **Entra ID** → **App registrations** → **New registration**. Nome livre.
   Anote o **Application (client) ID** e o **Directory (tenant) ID**.
2. Dentro dele, **Certificates & secrets** → **New client secret**. **Copie o
   valor agora**: o portal não o mostra de novo. Anote também a validade — um
   segredo que expira derruba a assinatura sem aviso, e a data é a única coisa
   deste documento que tem prazo.
3. Volte à conta de assinatura → **Access control (IAM)** → **Add role
   assignment** → o papel cujo nome termina em **Certificate Profile Signer**
   (a Microsoft o chamava de "Trusted Signing Certificate Profile Signer" antes
   da troca de nome do serviço) → atribua ao app registration criado.

O passo 3 é o que costuma faltar: sem ele o app registration existe, autentica, e
recebe "acesso negado" na hora de assinar.

### 2.6 — os seis segredos

Em **Settings → Secrets and variables → Actions**:

| segredo | onde estava |
|---|---|
| `AZURE_ENDPOINT` | `https://<região>.codesigning.azure.net`, do passo 2.2 |
| `AZURE_ARTIFACT_SIGNING_ACCOUNT` | o nome da conta, do passo 2.2 |
| `AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE` | o nome do perfil, do passo 2.4 |
| `AZURE_CLIENT_ID` | Application (client) ID, do passo 2.5 |
| `AZURE_CLIENT_SECRET` | o valor do client secret, do passo 2.5 |
| `AZURE_TENANT_ID` | Directory (tenant) ID, do passo 2.5 |

**Os três primeiros juntos ou nenhum.** Com apenas parte deles, o workflow avisa
e sai sem assinar, em vez de quebrar no fim do build.

### 2.7 — o que esperar

Assinado, o instalador para de ser "publicador desconhecido". O SmartScreen pode
ainda mostrar o aviso nas primeiras semanas: ele também pesa reputação, e
reputação se acumula com downloads. O que a assinatura garante é que a reputação
passa a se acumular **na sua identidade**, em vez de recomeçar do zero a cada
versão — que é a situação de hoje.

---

## Parte 3 — assinatura do macOS (Apple)

Esta parte o `release.yml` já esperava antes deste trabalho; os segredos estão lá
desde então. Resumo do que preencher:

1. Conta paga no **Apple Developer Program** (99 dólares por ano).
2. Certificado **Developer ID Application** — criado no portal, instalado no
   Chaveiro, exportado como `.p12` com senha.
3. `base64 -i certificado.p12 | pbcopy` → é isso que vai no segredo.
4. Uma **senha específica de app** em appleid.apple.com, para a notarização.

| segredo | conteúdo |
|---|---|
| `APPLE_CERTIFICATE` | o `.p12` em base64 |
| `APPLE_CERTIFICATE_PASSWORD` | a senha da exportação |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Nome (TEAMID)`, como aparece no Chaveiro |
| `APPLE_ID` | o e-mail da conta |
| `APPLE_PASSWORD` | a senha específica de app |
| `APPLE_TEAM_ID` | o identificador de dez caracteres |

**Não remova o `signingIdentity: "-"` do `tauri.conf.json` achando que ele foi
substituído.** Ele é assinatura ad-hoc, e é o que faz a permissão de microfone
grudar no macOS; o workflow o troca pela identidade de verdade quando o segredo
existe, e o mantém quando não existe. Sem ele — nem ad-hoc nem real — o macOS
pergunta pelo microfone a cada abertura e nunca lembra da resposta.

---

## Empacotar à mão, com as chaves

Quando a cota de Actions acaba — já aconteceu —, `empacotar/` faz o mesmo. Para
que o pacote saia atualizável, exporte a chave antes:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/seele.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='a senha'

./empacotar/macos.sh 0.1.2
./empacotar/linux.sh 0.1.2
```

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content ~\.tauri\seele.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'a senha'

.\empacotar\windows.ps1 -Versao 0.1.2
```

Depois de reunir os três sistemas na mesma pasta `entrega/`, monte o manifesto —
**este passo é o que o CI faria por você, e esquecê-lo deixa todo mundo sem
atualização até o release seguinte**:

```sh
python3 empacotar/manifesto.py entrega v0.1.2
```

Suba tudo o que estiver em `entrega/`, `latest.json` inclusive.

A assinatura do Windows pelo Azure **não** acontece no caminho manual: ela
depende do `signCommand` que só o workflow escreve. Um instalador feito à mão sai
sem ela, e é o comportamento certo — a alternativa seria pôr a credencial do
Azure na máquina de quem empacota.

---

## Resumo dos segredos

| segredo | para quê | sem ele |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | assina o pacote de atualização | não há botão de atualizar |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | abre a chave acima | idem |
| `AZURE_ENDPOINT` | assinatura do Windows | SmartScreen reclama |
| `AZURE_ARTIFACT_SIGNING_ACCOUNT` | idem | idem |
| `AZURE_ARTIFACT_SIGNING_CERTIFICATE_PROFILE` | idem | idem |
| `AZURE_CLIENT_ID` | o CI entra no Azure | idem |
| `AZURE_CLIENT_SECRET` | idem | idem |
| `AZURE_TENANT_ID` | idem | idem |
| `APPLE_CERTIFICATE` | assinatura do macOS | Gatekeeper reclama |
| `APPLE_CERTIFICATE_PASSWORD` | idem | idem |
| `APPLE_SIGNING_IDENTITY` | idem | idem |
| `APPLE_ID` | notarização | idem |
| `APPLE_PASSWORD` | idem | idem |
| `APPLE_TEAM_ID` | idem | idem |

Nenhum deles é obrigatório para o build passar. Todos entram vazios sem quebrar
nada — é a mesma regra em todos, e está num passo só do `release.yml`, o
«Gravar a versão e o que houver de credencial».
