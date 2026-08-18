# 0026 — Duas assinaturas, e um botão de atualizar

Status: aceito

## Contexto

Duas queixas do parceiro humano, depois de testar em duas máquinas:

- «Windows com erro com o controle inteligente, precisamos urgentemente assegurar a confiabilidade do sistema pra isso não ser problema.» O "controle inteligente" é o SmartScreen, e ele reclama porque **nada aqui é assinado** — as `NOTAS-DE-RELEASE.md` já explicavam isso e mandavam clicar em "Executar assim mesmo".
- «Botão de atualizar para não precisar ficar baixando o exe no github toda vez.» Isso já custou um teste real: as duas máquinas ficaram em versões diferentes, porque atualizar exigia baixar um instalador à mão em cada uma.

As duas são a mesma decisão: **este produto tem uma identidade, e ela assina o que publica.** Sem identidade não há assinatura de código, e sem assinatura não há canal de atualização defensável — um atualizador sem verificação é a porta mais barata que existe para comprometer um produto instalado.

O que já havia: `signingIdentity: "-"` no macOS (ad-hoc, que não vale para o Gatekeeper mas é o que faz a permissão de microfone grudar), os segredos da Apple entrando vazios no workflow, e atestado de procedência do GitHub em cada arquivo. Windows não tinha nada.

## Decisão

**São duas assinaturas, e confundi-las é o erro a evitar.**

1. **Assinatura de código do sistema operacional** — o que faz o sistema deixar abrir. macOS pela Apple (Developer ID + notarização); Windows pelo **Azure Artifact Signing**, o nome novo do Trusted Signing: na nuvem, sem token físico, na ordem de dez dólares por mês. Linux não pede nenhuma.
2. **Assinatura do projeto sobre o pacote de atualização** — o que faz o app recusar um pacote que não veio de nós. É uma chave `minisign` própria, e o plugin `updater` do Tauri a exige. Uma não substitui a outra: um instalador com Authenticode válido continua sendo qualquer instalador, e é o `latest.json` que diz qual baixar.

**Nenhuma delas bloqueia o build.** Segredo vazio significa seguir sem, exatamente como os da Apple já faziam. Um passo do workflow decide as três coisas — identidade da Apple, `signCommand` do Windows, `createUpdaterArtifacts` — pela mesma regra: existe o segredo? então ligue.

**`createUpdaterArtifacts` fica desligado no repositório**, e um teste em `tests/empacotamento.rs` o mantém desligado. Ligado no arquivo comitado, a CLI do Tauri passa a exigir a chave privada de quem quer que rode `cargo tauri build` — e falha depois de compilar tudo.

**O manifesto mora no próprio release**, em `releases/latest/download/latest.json`. Para o GitHub, `latest` é o último release **publicado e não pré-lançamento**: rascunho não conta. É essa propriedade que mantém a decisão de lançar onde ela sempre esteve — enquanto uma pessoa não publicar o rascunho à mão, nenhum app enxerga a versão nova. Não há serviço a hospedar para isso valer.

**Quem decide é a pessoa.** Dois comandos, `procurar_atualizacao` e `instalar_atualizacao`, e nenhuma consulta automática ao abrir. Num produto cujo argumento é que o servidor é seu, um app que fala com o github.com a cada arranque contradiz o argumento — e o que foi pedido foi um botão.

## Alternativas

1. **Certificado OV/EV comum para Windows.** Entre 200 e 600 dólares por ano, com token físico USB no caso do EV — impossível de usar num runner. E OV não zera o SmartScreen: ele constrói reputação ao longo de semanas de downloads.
2. **Não assinar e continuar explicando o alerta.** É o que está feito hoje, e as notas de release fazem isso bem. Deixou de bastar quando a queixa veio de quem estava instalando.
3. **Atualização silenciosa.** Menos passos e mais rápida a convergir. Descartada: num app de conversa, um binário trocado sem aviso é intrusivo, e no Windows a troca fecha a janela — fechar a janela de alguém sem perguntar não é atualizar, é interromper.
4. **Servidor de atualização próprio.** Daria controle sobre faseamento e reversão. Custa uma máquina a mais para manter, num projeto que ainda não tem a primeira, e o release do GitHub já responde a pergunta.
5. **Sem chave de atualização, confiando no HTTPS.** É o erro clássico. TLS diz de qual servidor o arquivo veio, e não quem o produziu — uma conta comprometida ou um release adulterado passam intactos. A assinatura do pacote é conferida contra uma chave que está dentro do app, e é a única parte que um invasor não alcança pela rede.

## Consequências

- **Duas chaves para uma pessoa guardar**, e a privada do atualizador é a que não tem substituto: perdê-la significa que nenhum app instalado aceitará atualização até que todo mundo reinstale à mão. `docs/assinatura-e-atualizacao.md` é o passo a passo, e diz isso com todas as letras.
- **A página de release ganha arquivos que não são para pessoas** — `.sig` e `latest.json`, mais um `.app.tar.gz` no macOS. As notas de release passam a dizer o que são, porque «um arquivo por sistema» continua sendo o combinado para quem chega.
- **Falha no meio da atualização não deixa meia instalação.** O pacote é baixado inteiro para a memória e a assinatura é conferida **antes** de qualquer arquivo instalado ser tocado. Rede que cai, download truncado, pacote adulterado: os três terminam com o app exatamente como estava.
- **Instalar fecha e reabre o SEELE**, nos três sistemas. No Windows não há escolha — o instalador do NSIS não roda com o programa aberto. Nos outros dois o processo continua vivo rodando o código antigo, e reabrir é o que faz a atualização valer. Uniformizado porque uma ação que às vezes fecha a janela é uma ação que ninguém consegue avisar direito.
- **O caminho sem CI continua completo.** `empacotar/manifesto.py` é o único lugar onde a regra do manifesto mora, e o workflow chama o mesmo arquivo. Um release montado à mão sem manifesto deixaria todo mundo parado até o seguinte, e quem o montou não teria como saber.
- **Uma dependência a mais na árvore do desktop**, e ela não custou exceção nenhuma no `deny.toml`: `reqwest`, `rustls` e `ring` já estavam lá. O que entra de novo é `minisign-verify`, que é pequeno e é exatamente o que confere a assinatura.

Custo de reverter: **baixo** enquanto ninguém tiver atualizado por aqui. Depois do primeiro release assinado, trocar de chave exige que todo mundo reinstale à mão — que é o mesmo custo de perdê-la.

---

## O que a primeira pessoa a apertar o botão ensinou

O botão foi apertado num app de verdade, num release que ainda não existia, e
respondeu **«a página de releases não respondeu»**. A página tinha respondido, e
respondido corretamente: não havia nada publicado.

O erro estava no agrupamento. `ReleaseNotFound` foi escrito junto de
`Reqwest`, `Network` e `Serialization` sob a mesma variante `NaoAlcancei`,
porque os quatro são «não consegui o manifesto». São — e é a pergunta errada. A
pergunta certa é **o que a pessoa faz a seguir**, que é o critério que este
próprio ADR usou para escrever seis variantes em vez de uma:

- rede que falhou: **tente de novo daqui a pouco**;
- página que respondeu «nada publicado»: **não adianta tentar de novo**, e a sua
  conexão está boa.

Mandar conferir a conexão por causa do segundo manda a pessoa procurar defeito
onde não há. Então são **sete** variantes, e a nova é `NadaPublicado`.

Vale registrar o padrão, porque este ADR é o segundo a cair nele no mesmo dia. O
ADR 0022 teve exatamente o mesmo conserto: o ponto de encontro padrão, que
ninguém publicou ainda, dizia «o nome não resolve, ou esta máquina está sem
DNS» — culpando a máquina de quem hospeda por uma pendência nossa.

**As duas vezes o defeito foi o mesmo:** uma pendência de infraestrutura nossa
apareceu na tela vestida de falha da máquina de quem estava usando. É um viés
com direção — quem escreve a frase sabe que o serviço ainda não existe, e por
isso não pensa nele como uma das causas possíveis. Quem lê não sabe, e assume a
única causa que a frase oferece.

O teste que conta as variantes é o que obriga a próxima a vir acompanhada de
frase e de motivo, em vez de chegar calada e dividir a redação de outra.
