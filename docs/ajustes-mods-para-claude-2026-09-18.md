# Ajustes de MODs para o Claude — 18/09/2026

## Escopo

Solicitação do usuário. Este documento especifica mudanças no SEELE e no fluxo de MODs; não aplica correções. Os relatos abaixo são testes de campo do usuário, não reproduções feitas nesta rodada. Confirmar versão e hash do aplicativo, dos pacotes e do conjunto exigido antes de diagnosticar.

O MOD PERFIS foi ajustado separadamente pelo Codex na versão 1.1.0: substituição da apresentação lateral e aumento de limites de avatar/banner. Não duplicar essa implementação nesta tarefa. A validação no aplicativo nativo ainda está pendente; testes de servidor, interface em navegador e QuickJS foram executados.

## 1. Consentimento deve levar a instalação e ativação dos MODs exigidos

Ao entrar em um servidor e aceitar seus MODs, o convidado deve ter os pacotes correspondentes instalados/verificados e carregados automaticamente. Não deve precisar descobrir outra tela para instalar ou ligar cada MOD. Enquanto estiver naquele servidor, não pode desligar individualmente um MOD obrigatório e continuar na sessão como se cumprisse o conjunto exigido.

O aceite precisa explicar que autoriza download e execução daquele conjunto exato. Antes dele, não executar código de terceiros. Uma cópia local já existente só pode ser reutilizada se corresponder ao hash requerido; aceitar não equivale a confiar em qualquer versão futura.

Estados visíveis: aguardando consentimento → obtendo pacotes → verificando → carregando → pronto. Falhas precisam identificar MOD, etapa e próximo passo. Não considerar “instalado” ou “aceito” como prova de execução do cliente.

Se o pacote exato estiver indisponível, revogado ou incompatível, não substituir silenciosamente pela versão mais nova nem baixar de fonte arbitrária. Informar o impedimento e oferecer nova tentativa ou saída. Preservar a validação de procedência, integridade e compatibilidade prevista pelo SEELE.

Na gestão do convidado, mostrar “Exigido por este servidor”, não um switch local de desligamento. Sair do servidor encerra execução, remove alterações de interface e restaura o estado local; não exige apagar o pacote baixado. Em outro servidor, só executar o conjunto daquele destino.

### Falha relatada: estilo não aplicado ao convidado

O usuário entrou como convidado em um servidor com MOD de estilo, mas a cor do servidor não mudou, apesar de ser a finalidade do MOD.

Investigar separadamente: pacote/hash local, aceite, carregamento do cliente, permissões de leitura do estado, obtenção do tema do servidor, aplicação dos tokens e limpeza ao trocar de servidor. Edição pode ser exclusiva do host; leitura/aplicação do tema precisa funcionar para o convidado. Não atribuir causa antes de observar cada etapa.

### Critérios de aceite

- Cliente sem MOD local aceita o conjunto e chega à sessão com o estilo publicado pelo host, sem passos manuais extras.
- Cliente com outra versão recebe/reutiliza somente o conteúdo exato exigido.
- Convidado não consegue desligar um MOD obrigatório durante a sessão; pode sair ou recusar a entrada.
- Falha de download/hash/runtime aparece na interface, não apenas no console.
- Recusa não executa MOD; trocar de servidor ou sair restaura cores e elementos alterados.
- Validar host + convidado em macOS e Windows, usando aplicativo empacotado, não apenas preview HTTP.

## 2. Switches editam um rascunho; SALVAR aplica o conjunto uma vez

Hoje, segundo o teste do usuário, cada ativação expulsa o host da sessão e exige nova entrada. Ativar vários MODs repete o processo para cada um.

Fluxo solicitado: switches LIGAR/DESLIGAR alteram apenas uma seleção pendente. O servidor continua com seu conjunto atual até o operador pressionar **SALVAR ALTERAÇÕES**. Mostrar alterações pendentes, botão DESCARTAR e aviso de saída com edições não salvas. Sem diferenças, SALVAR fica desabilitado.

SALVAR deve validar todos os pacotes e aplicar o conjunto como uma única operação atômica. Não implementar como um laço de chamadas aos comandos atuais se cada chamada já notifica mudanças e encerra sessões. Uma falha de validação/gravação mantém o conjunto anterior inteiro e preserva o rascunho para correção.

O objetivo é **no máximo um ciclo de reconexão/novo aceite por aplicação**, não um por switch. Preferencialmente, a ação informada de salvar já cobre o consentimento do operador local para o conjunto resultante, evitando uma confirmação equivalente na sequência. Isso não autoriza alterações futuras nem elimina o aceite de convidados.

Explicar antes de salvar o efeito sobre participantes. Se for necessária reconexão local, preservar a hospedagem; desconectar o cliente do host não deve encerrar a escuta ou tentar voltar para um servidor que o próprio fluxo desligou.

Prever conflito: se outro operador/processo alterou o conjunto desde que a tela abriu, não sobrescrever silenciosamente. Comparar revisão/hash-base e pedir atualização/reconciliação. Impedir duplo envio durante SALVAR.

### Critérios de aceite

- Alternar três switches não modifica o servidor nem derruba ninguém antes de SALVAR.
- Salvar as três mudanças produz um conjunto final e no máximo uma transição de sessão por participante.
- DESCARTAR e cancelar a saída não publicam mudanças; salvar sem mudanças não produz queda.
- Um MOD inválido impede a aplicação inteira; o conjunto anterior permanece ativo.
- Host mantém hospedagem e não repete autenticação/aceite equivalente desnecessariamente.
- Convidados recebem o novo conjunto final, não estados intermediários.
- Duplo clique e edição concorrente não publicam duas vezes nem perdem alterações silenciosamente.

## Entrega esperada

Implementar com testes de comportamento, registrar as versões realmente testadas e demonstrar os dois percursos: convidado novo recebe/aplica estilo; host altera vários MODs e salva uma vez. Não confundir publicação do pacote, instalação local, consentimento, ativação no servidor e inicialização do cliente: são etapas distintas que o fluxo precisa coordenar.
