# Usar o SEELE 0.18

Este guia descreve a versão 0.18. Confira a versão disponível em [Downloads](https://get.seele.sh/#baixar). O [guia anterior](usar-a-0.17.md) registra o comportamento até a 0.17.3.

## Instalar

No computador, baixe a versão publicada em [Downloads](https://get.seele.sh/#baixar). O navegador abre em [seele.sh](https://seele.sh/). As mudanças abaixo dependem do app, da página e do ponto de encontro compatíveis com a 0.18; abrir o endereço público não comprova que esse lote já foi implantado.

## Conversar pelo código

Na 0.18, a conversa pessoal por código funciona nos dois sentidos: navegador chama desktop e desktop chama navegador. Navegador e desktop também podem chamar outro participante da mesma plataforma. O código identifica a pessoa; o nome serve para apresentação.

1. Abra o SEELE e confira seu código pessoal.
2. Habilite receber chamadas. No navegador, mantenha a página aberta e confira o estado da recepção.
3. Informe o código da outra pessoa na ação de chamar. Quem recebe precisa aceitar antes de abrir a conversa direta.
4. Para sair, encerre a conversa. Uma nova chamada exige outro aceite.

Um convite de conversa pessoal também pode ser compartilhado por link. Ele tem prazo e pode ser revogado. Esse convite é diferente do link para entrar em um servidor com salas e canais.

## Amigos e contatos

Adicione uma pessoa aos amigos pelo código. Você pode dar um nome local ao contato, removê-lo dos amigos ou bloqueá-lo. Pessoas com o mesmo nome continuam distintas pelo código. Adicionar aos amigos não concede acesso a servidores nem atende chamadas automaticamente.

A agenda e os amigos ficam no dispositivo ou no armazenamento daquela origem do navegador. Abrir o SEELE no navegador não importa a identidade nem a agenda do desktop. Os códigos dessas identidades podem ser diferentes.

## Recepção e falhas de rede

A página precisa permanecer aberta e apta a executar para receber pelo código. Fechar a aba, suspender o navegador ou deixar o sistema interromper a página pode tornar a pessoa indisponível. Não há promessa de toque com a página fechada.

O código permanece associado à identidade enquanto ela existir. O registro temporário de recepção é renovado; ele não transforma o código em um serviço sempre disponível. Se a recepção indicar indisponibilidade, confira a rede e tente habilitar o recebimento novamente. As tentativas automáticas de recuperação são limitadas.

Uma falha do ponto de encontro não encerra, por si só, uma conversa direta já estabelecida. Se a conexão de mídia cair, ela não é reconectada automaticamente: encerre e faça uma nova chamada, com novo aceite. Recuperar a recepção não reconecta a mídia de uma conversa encerrada.

## Identidade e armazenamento

No navegador, a chave pessoal não exportável e a agenda persistem no IndexedDB da origem, quando o armazenamento está disponível. Recarregar normalmente preserva o código. Apagar os dados do site, perder esse armazenamento ou usar outra origem pode criar outra identidade e outro código, sem recuperar a agenda anterior.

A primeira chave completa observada para um contato fica fixada. A assinatura e a correspondência entre código e chave são conferidas antes da chamada. Uma chave divergente ou um contato bloqueado é recusado; um nome igual não substitui a identidade.

## Entrar num servidor

Receba o link de quem hospeda, confira o servidor e entre com seu apelido. O navegador pode participar de servidores e aceitar os MODs recebidos deles. O desktop continua necessário para hospedar, administrar a portaria, criar salas e canais e instalar ou gerir os MODs do servidor.

## Hospedar

No computador, escolha hospedar aqui. Sua máquina mantém o servidor disponível enquanto estiver ligada e o SEELE estiver funcionando. Compartilhe o convite do servidor com o grupo. As permissões de cada pessoa dependem dos cargos definidos nesse servidor.

## Voz e tela

Autorize o microfone quando quiser usar voz e confira os controles de mudo e saída. O áudio vai para o contexto escolhido; sair ou trocar de contexto deve respeitar os controles da interface. Use fones para evitar que o som do alto-falante volte pelo microfone.

Compartilhar tela depende da plataforma e das permissões disponíveis. O navegador do celular não oferece o compartilhamento de tela deste lote. O guia não promete câmera, resolução, taxa de quadros ou desempenho iguais em todas as plataformas.

No caminho alternativo de compartilhamento do WebKit, a tela segue como vídeo, sem som do sistema. A voz continua em seu canal separado; compartilhar a tela não transmite automaticamente o áudio do computador.

## Alcance pela internet

Não há relay nem servidor TURN. O ponto de encontro encaminha a sinalização para apresentar as pontas; texto e mídia da conversa seguem pelo transporte direto depois do aceite. Se as redes não permitem esse caminho, a conexão pode falhar. O código não contorna essa limitação.

## Convites antigos

Endereços `seele.app.br` e seus subdomínios foram aposentados. Peça um convite novo em `seele.sh`. Servidores já guardados podem atualizar o endereço do encontro localmente, preservando a identidade e a impressão conhecidas. A identidade do navegador no domínio antigo não é transferida.

## Referências

O [guia até a 0.17.3](usar-a-0.17.md) e as [notas da 0.17.0](../empacotar/notas/0.17.0.md) permanecem como histórico. A identidade e os códigos têm suas decisões nos ADRs [0060](adr/0060-o-codigo-de-uma-pessoa.md) e [0061](adr/0061-a-identidade-e-uma-por-maquina.md). A documentação da [API dos MODs](../api/README.md) continua separada da conversa pessoal.
