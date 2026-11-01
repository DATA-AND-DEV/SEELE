# Usar o SEELE 0.19

Este guia descreve a versão 0.19. Confira a versão disponível em [Downloads](https://get.seele.sh/#baixar). O [guia anterior](usar-a-0.18.md) registra o comportamento da 0.18.

## Instalar

No computador, baixe a versão publicada em [Downloads](https://get.seele.sh/#baixar). Os pacotes da 0.19.0 são para macOS Apple Silicon e Windows x64; o Linux não foi empacotado nesta rodada. O navegador abre em [seele.sh](https://seele.sh/).

A 0.19 traz uma interface nova no aplicativo, na web e no celular: telas redesenhadas, ícones no lugar de textos, cartões de pessoa mais simples e um modal para ver cada pessoa.

## Conversar pelo código

A conversa pessoal por código segue como na 0.18: navegador e desktop chamam um ao outro, e cada plataforma chama a mesma plataforma. O código identifica a pessoa; o nome serve para apresentação.

1. Abra o SEELE e confira seu código pessoal.
2. Habilite receber chamadas. No navegador, mantenha a página aberta e confira o estado da recepção.
3. Informe o código da outra pessoa na ação de chamar. Quem recebe precisa aceitar antes de abrir a conversa direta.
4. Para sair, encerre a conversa. Uma nova chamada exige outro aceite.

Na 0.19, chamadas, a sala de voz, a conexão e a portaria têm avisos sonoros. O pedido de microfone vem ao entrar na chamada, e um microfone que falha não derruba mais a escuta.

## Amigos e contatos

Adicione uma pessoa aos amigos pelo código. Você pode dar um nome local ao contato, removê-lo dos amigos ou bloqueá-lo. Adicionar aos amigos não concede acesso a servidores nem atende chamadas automaticamente.

A agenda fica no dispositivo ou no armazenamento daquela origem do navegador, como na 0.18. Abrir o SEELE no navegador não importa a identidade nem a agenda do desktop.

## Seu perfil

Na 0.19, o nome, a cor e a imagem do perfil passam a ser do SEELE, e não mais do MOD PERFIS. Ao atualizar, os perfis do MOD PERFIS são migrados para o produto. Se alguma imagem não puder vir, o app avisa; o resto do perfil é migrado assim mesmo.

Um servidor 0.19 desliga o MOD PERFIS e não o liga de novo. No catálogo, o MOD foi descontinuado, e não revogado: quem ainda está num servidor 0.18 continua podendo usá-lo.

## Entrar num servidor

Receba o link de quem hospeda, confira o servidor e entre com seu apelido. Quem espera a aprovação de quem hospeda vê que está só esperando. Sair dessa tela, ou fechar a janela, pede confirmação.

O navegador pode participar de servidores e aceitar os MODs recebidos deles. Um MOD pode declarar que só roda no app de computador; quem entra pelo navegador não carrega esse MOD, e a tela de MODs diz isso.

## Hospedar

No computador, escolha hospedar aqui. Uma tela de carregamento aparece enquanto o servidor sobe. Sua máquina mantém o servidor disponível enquanto estiver ligada e o SEELE estiver funcionando. Compartilhe o convite com o grupo. As permissões de cada pessoa dependem dos cargos definidos nesse servidor.

O desktop continua necessário para hospedar, administrar a portaria, criar salas e canais e instalar ou gerir os MODs do servidor. Os modais de MOD seguem o estilo dos modais do app, e a barra de controles fica sempre livre.

## Jogos multiplayer (API 8)

A API 8 dos MODs permite que um MOD hospede um servidor de jogo no computador de quem hospeda, com a aprovação dele. O primeiro MOD que usa isso é o [Multiplayer](https://github.com/DATA-AND-DEV/SEELE-MOD-MULTIPLAYER) (`seele/multiplayer` 1.0.0), com o Minecraft Vanilla e com modpack do Modrinth.

1. Instale o MOD no servidor pelo app de computador. Ele só roda no app, e não no navegador.
2. Antes de qualquer download, o SEELE mostra, numa tela do produto, o que o MOD pede: o que vai ser baixado, de onde, e o que vai rodar. Quem hospeda aceita ou recusa, na própria máquina. Nada é baixado nem roda antes do aceite.
3. Com o jogo de pé, quem joga usa o botão de entrar do MOD. O app abre uma porta local e mostra qual é; o jogo se conecta por `localhost:<porta>`. Uma faixa do produto, fora da tela do MOD, diz que a porta está aberta e para qual jogo.
4. O jogo para quando o servidor do SEELE para. Ao reabrir o app depois de uma queda, um jogo que ficou para trás é salvo e parado antes de qualquer servidor subir de novo.

O que foi testado: a hospedagem pela API 8 no app de verdade, com Forge e NeoForge, no Mac e no Windows. Subir, entrar, salvar, reiniciar o servidor e fechar o app com o jogo de pé.

O que não foi testado:

- a hospedagem de jogo entre duas casas diferentes, e um cliente Forge entrando por conta Microsoft;
- Linux e Quilt com a API 8;
- MODs de navegador que abrem porta.

No Windows, encerrar a sessão do sistema dá ao jogo só cerca de 1 segundo para salvar antes de o Windows fechá-lo. Pare o servidor de jogo antes de sair da sessão.

O túnel do jogo usa a mesma ligação do SEELE. Sem relay, quem o SEELE não conecta o jogo também não conecta.

## Voz e tela

Autorize o microfone quando quiser usar voz e confira os controles de mudo e saída. Use fones para evitar que o som do alto-falante volte pelo microfone.

Compartilhar tela depende da plataforma e das permissões disponíveis. O navegador do celular não oferece o compartilhamento de tela. O congelamento ao compartilhar a tela do Mac numa conversa pessoal continua sem correção na 0.19.

## Alcance pela internet

Não há relay nem servidor TURN. O ponto de encontro encaminha a sinalização para apresentar as pontas; texto e mídia seguem pelo transporte direto depois do aceite. Se as redes não permitem esse caminho, a conexão pode falhar. O código não contorna essa limitação.

## MODs de versões anteriores

Os MODs das APIs 6 e 7 continuam rodando na 0.19 sem ser republicados. Um MOD da API 8 não roda na 0.18: o catálogo só oferece à 0.18 as versões que ela executa.

## Referências

O [guia da 0.18](usar-a-0.18.md) e as [notas da 0.19.0](../empacotar/notas/0.19.0.md) registram o que mudou. A documentação da [API dos MODs](../api/README.md) descreve a API 8 por inteiro, e o guia [Criar um MOD](https://docs.seele.sh/mods/) explica a API 8 para quem escreve MOD.
