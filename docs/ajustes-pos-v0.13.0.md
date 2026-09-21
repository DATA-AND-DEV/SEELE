# Ajustes após a v0.13.0

Implementação local em SEELE, SEELE-MOD-ESTILO, SEELE-MOD-PERFIS e SEELE-MOD-MESA. Não houve publicação de release ou alteração dos dados da instalação usada pela pessoa.

## Comportamento

- A gestão de MODs aparece apenas nas configurações de uma sessão conectada. Uma seção lembrada não reaparece na tela inicial.
- Foram removidas explicações estáticas repetidas das configurações e da gestão de MODs. Estados, erros, consentimento e consequências de exclusão continuam visíveis.
- A lista de instalados oferece **APAGAR MOD**. É necessário desligar e salvar antes; o backend também verifica referências dos demais servidores locais. Dados dos MODs não são apagados junto com o pacote.
- A lista de hospedagem oferece **APAGAR SERVIDOR**, com confirmação e hospedagem parada. Apaga o banco e os dados daquela instância; preserva os outros servidores, os pacotes e a identidade TLS da máquina. Servidores visitados têm **REMOVER** explícito.
- Imagens de MODs aceitam até **10 MiB** na seleção e na leitura. O orçamento de exibição comporta imagens desse tamanho. A MESA também grava e lê imagens em partes, mantendo leitura do formato antigo.
- `SeeleUI.superficies.criar({ tipo: "pagina", imersiva: true, ... })` ocupa a sessão e mantém saída, Escape, foco e restauração. MESA usa essa opção.
- `listarNaBarra: false` em uma contribuição `servidor.navegacao` oculta a entrada. O padrão permanece `true`. ESTILO usa o resultado autenticado `canEdit`; o tema continua aplicado aos participantes. Visibilidade não substitui autorização do servidor.
- Compartilhamento oferece **540p, 720p e 1080p**, inclusive alteração durante a transmissão. As hipóteses iniciais de capacidade são 3, 5 e 8 Mbps, respectivamente; com reserva de voz de 40%, os tetos locais iniciais correspondem a 1,8, 3 e 4,8 Mbps, antes das restrições do servidor/espectadores. Uma medida lembrada prevalece. A escada continua adaptativa. O arranque não medido do servidor passa a 8 Mbps.
- Avisos de MODs mostram o texto e expiram (3,5 s normal, 10 s erro). Descartar também libera o registro e o temporizador. ESTILO deixa de emitir a confirmação flutuante redundante de publicação.

## Falha de foto e banner no PERFIS

O log local de 21/09, às 04:06 UTC, registra `control frames over budget`, seguido por pedidos de `seele/perfis` sem resposta até o timeout. O envio usa fragmentos base64 em `ModRequest`, pelo controle, diferente do transporte de anexos. Cada fragmento podia ser enviado tão logo a resposta anterior chegasse; isso ultrapassava o balde de 20 quadros/s do servidor mesmo abaixo de 1 MB.

A ponte agora distribui **oito pedidos de MOD por segundo**, compartilhados por todos os MODs, uploads e leituras. A espera ocorre antes do prazo de resposta e cancela pedidos de sessões encerradas. O servidor mantém seus limites. PERFIS mostra o estado de envio, impede uploads concorrentes de foto/banner e libera o arquivo escolhido inclusive quando `upload-start` falha.

O número de **4 MB** vinha do seletor reutilizar o orçamento de mídia da região para limitar o campo de arquivo. Havia ainda um teto de **1 MiB** na conversão nativa para exibição e um máximo de continuações insuficiente para 10 MiB. Esses limites foram alinhados. O limite antigo de imagens da MESA era outro problema, independente do relato do PERFIS.

Este ajuste mantém o transporte fragmentado do PERFIS. Não migra uploads de MODs para o fluxo binário de anexos; arquivos grandes ainda exigem muitas viagens pelo controle.

## Validação

- `cargo check -p seele-app`.
- `cargo test -p seele-app --test frontend`: 237 testes.
- `cargo test -p seele-app --bin seele-app servidores::`: 17 testes, incluindo exclusão isolada e identidade TLS após apagar o legado.
- `cargo test -p seele-core --lib tela::`: 44 testes.
- `cargo test -p seele-server --lib tela::`: 29 testes.
- `cargo test -p seele-core --lib caminho`: 24 testes.
- `npm test` nos três MODs: PERFIS 40, ESTILO 23, MESA 50, incluindo upload e leitura de 10 MiB para foto/banner e mapa/retrato.
- Bancadas `regiao-do-mod.cjs`, `contribuicoes-e-camadas.cjs`, `continuacao-de-midia.cjs` e `envio-de-imagens.cjs`.
- `ajustes-v013.cjs`: Chromium com HTML/CSS reais e ponte simulada. Verifica gestão dentro/fora da sessão, resolução aplicada durante transmissão, navegação oculta, largura imersiva/Escape e expiração/liberação de avisos.

A validação de navegador não substitui uma chamada entre dois aplicativos nativos; a qualidade visual dos novos arranques de vídeo em rede real ainda requer essa verificação.
