# Reteste do SEELE — servidor, instalação e atualização de MOD

Data: 17/09/2026, aproximadamente 21:57–22:04 (America/Sao_Paulo).
Aplicativo: `/Applications/SEELE.app`, versão 0.11.0, macOS.
Servidor criado: `QA MOD 17-09`. MOD: `seele/mesa`, versões 1.2.0 e 1.2.1.
Código consultado para diagnóstico: checkout `7c8c0c5`. A equivalência integral desse checkout com o binário instalado não foi verificada.

## Resultado

O fluxo foi executado na interface nativa. Criar servidor e instalar por pasta funcionaram. Atualizar pelo catálogo baixou a nova versão, mas deixou o servidor exigindo a versão anterior. A aprovação duplicada do host foi reproduzida tanto ao ativar 1.2.0 quanto ao ativar 1.2.1 após a atualização. O aperto visual do botão ATUALIZAR também foi observado.

Este é um reteste dos fluxos solicitados. Não constitui aprovação de todas as telas, plataformas ou funcionalidades do SEELE.

## Casos executados

| Caso | Resultado observado |
|---|---|
| Hospedar aqui → Novo servidor → nome → Hospedar | Servidor criado, sessão conectada e convite exibido. |
| Buscar no catálogo | Catálogo retornou Mesa 1.2.1 e reconheceu o pacote inicialmente instalado. |
| Ligar Mesa 1.2.1 inicialmente instalado | Confirmação seguida de desconexão; reconexão sem novo aceite nessa primeira tentativa. Já existia um aceite local para o conjunto. |
| Desligar Mesa | Removeu a exigência e desconectou o host, exigindo reconexão manual. |
| Instalar Mesa 1.2.0 por pasta | Instalação concluída, versão 1.2.0 exibida e botão LIGAR disponível. |
| Ligar Mesa 1.2.0 → reconectar | Repetiu a aprovação, apesar da confirmação anterior prometer registrar o aceite. |
| Buscar catálogo com 1.2.0 instalado | Exibiu a versão disponível 1.2.1 e o botão ATUALIZAR. |
| Atualizar Mesa ligado, 1.2.0 → 1.2.1 | Download concluído; pacote novo no disco, exigência antiga no servidor e mensagem de sucesso incoerente. |
| Recuperar: desligar → reconectar → ligar → reconectar | Repetiu aprovação de 1.2.1. Depois de aceitar, abriu o Mesa com estado “Conectado · alterações confirmadas pelo servidor”. |
| Sair do servidor QA | Hospedagem encerrada e tela de entrada exibida. |

## Bugs e ajustes

### QA-01 — P1 — Atualização de MOD ativo deixa pacote e servidor incompatíveis

**Reprodução:** instalar 1.2.0, ligá-lo no servidor, aceitar e entrar; buscar o catálogo e clicar ATUALIZAR para 1.2.1.

**Observado às 22:02:** a gestão mostra 1.2.1, mas informa: “o que está instalado aqui não é o que o servidor exige — o servidor exige o conteúdo ab12b0645dc51fd9…”. O botão continua DESLIGAR. O servidor ainda exige o pacote antigo.

**Impacto:** o download bem-sucedido não conclui a atualização funcional do servidor. O usuário precisa descobrir uma sequência de desligar e ligar, com desconexões, para aplicar a nova versão.

**Correção sugerida:** tratar atualização de MOD ativo como uma operação própria, coordenando pacote, exigência do servidor, execução e consentimento para o novo conjunto. Alternativamente, impedir a substituição enquanto ativo e fornecer um fluxo explícito de desativação e atualização. A tela precisa indicar quando a versão está apenas instalada e quando está efetivamente aplicada.

**Indício no código:** `apps/seele-app/src/main.rs:2354` chama a instalação do catálogo sem receber a sessão hospedada; `apps/seele-app/src/catalogo.rs:421` substitui os arquivos, sem atualizar a exigência persistida.

### QA-02 — P2 — Host precisa aprovar novamente a mesma ativação

**Reprodução:** em servidor hospedado aqui, ligar um conjunto diferente do aceite anterior, confirmar “EXIGIR NESTE SERVIDOR E USAR NESTE COMPUTADOR” e reconectar.

**Observado às 22:00:40 e 22:03:07:** aparece “ESTE SERVIDOR USA MODS” e o botão “ACEITAR 1 MOD E ENTRAR”. A primeira confirmação havia prometido que o usuário não seria perguntado novamente pela decisão que acabou de tomar.

**Impacto:** pergunta duplicada e quebra da promessa do diálogo. A primeira tentativa com 1.2.1 não exibiu isso porque havia um aceite anterior compatível; esse cenário sozinho mascara o defeito.

**Causa provável identificada:** `apps/seele-app/src/main.rs:526` preenche `session.alvo` dentro de `if !hospedado_aqui(&alvo)`. Já `aplicar_mod`, na linha 2172, só grava o aceite se `session.alvo` estiver preenchido. A sessão local fica fora da condição necessária para a funcionalidade que deveria atender o host.

**Correção sugerida:** guardar o destino real da sessão também na hospedagem local, separando essa informação do histórico de servidores visitados. Registrar o consentimento explícito para o conjunto resultante antes da reconexão. Não basta dispensar consentimento genericamente por ser administrador.

**Validação necessária:** ativação inicial sem aceite, ativação com aceite antigo, atualização de versão e host que anteriormente visitou um servidor remoto.

### QA-03 — P2 — Sucesso da atualização afirma “desligado” quando está ligado

**Observado:** após atualizar, o catálogo mostra “seele/mesa 1.2.1 instalado, e desligado. Ligue quando quiser que ele valha.”, enquanto a lista de instalados apresenta DESLIGAR e a incompatibilidade da QA-01.

**Correção sugerida:** devolver o resultado real da operação à interface e distinguir instalação, atualização pendente de aplicação e atualização aplicada. O texto fixo está em `apps/seele-app/ui/camada-mods.js`, função `instalarDoCatalogo`.

### QA-04 — P3 — ATUALIZAR fica colado à barra de rolagem

**Observado às 22:01:** no cartão do catálogo, o botão fica muito próximo da barra vertical interna, com pouco espaço à direita. Continua visível e foi clicável; não foi constatado transbordamento completo. As capturas estão registradas na conversa.

**Impacto:** aparência apertada e leitura prejudicada por duas regiões de rolagem: a seção e a lista interna. Rolar o cartão também desloca seu conteúdo dentro de uma área curta.

**Ajuste sugerido:** reservar espaço estável para a barra, garantir margem interna para a ação e permitir que o botão passe para outra linha em larguras menores. Reavaliar a altura limitada da lista para um catálogo com apenas um item. Revisar `.mods-linha-gestao` e `.mods-rolagem` em `apps/seele-app/ui/camada-mods.css`.

### QA-05 — P2 — Catálogo mantém “JÁ INSTALADO” após instalar versão diferente por pasta

**Contexto:** o catálogo já tinha sido consultado quando 1.2.1 estava no disco. Depois da preparação controlada do teste, 1.2.0 foi instalada pela interface.

**Observado às 22:00:** a lista de instalados mostra 1.2.0, mas o catálogo continua marcando 1.2.1 como JÁ INSTALADO. Fechar e reabrir as configurações não resolveu. Buscar o catálogo novamente fez aparecer ATUALIZAR.

**Correção sugerida:** após instalação por pasta, redesenhar também o catálogo já carregado usando o inventário local atualizado. `instalarUmMod` chama `desenharMods`, mas não `desenharCatalogoEmMaos`.

## Melhorias adicionais de fluxo

- Depois de uma alteração explícita do host, reconectar automaticamente quando possível ou apresentar uma ação de continuação contextual. A tela ENLACE ENCERRADO trata uma consequência esperada da própria ação como uma interrupção genérica.
- A mensagem de incompatibilidade deve oferecer a ação de recuperação. Mostrar apenas o hash antigo não explica ao usuário como aplicar a versão instalada.
- Na primeira abertura de configurações pelo aviso de convite, o botão dizia VOLTAR À ENTRADA apesar de a sessão estar conectada. Nas aberturas seguintes dizia VOLTAR AO SERVIDOR. O primeiro botão não foi acionado; fica como inconsistência de rótulo observada, sem conclusão sobre seu comportamento.

## Preparação, restauração e limites

Para tornar a atualização testável, o pacote original Mesa 1.2.1 foi guardado fora de `mods/`; foi instalada pela interface a pasta oficial 1.2.0 existente em `SEELE-MODS-INDEXER/publicado/mods/seele/mesa/1.2.0`. O catálogo real forneceu a atualização 1.2.1.

Ao final, a hospedagem QA foi encerrada e o pacote original foi restaurado. Os 15 arquivos foram conferidos por SHA-256 contra o estado guardado. O pacote obtido no teste permanece em `/Users/dev-alexandre/.config/seele/qa-backups/20260917-215950/mesa-atualizada-no-teste`. O servidor QA permanece salvo, sem estar hospedado; o fluxo alterou o aceite local para o conjunto de MODs aprovado. Não foram modificados os servidores existentes nem o código do produto.

Não foram testados: perda de rede durante download, assinatura inválida, disco cheio, preservação de uma campanha com dados durante atualização, múltiplos clientes, todas as funções internas do Mesa ou outros sistemas operacionais. Não houve teste de áudio ponta a ponta.

A captura só voltou a funcionar depois que as janelas foram movidas para a tela integrada. Isso foi um contorno observado, não uma prova de que o monitor USB-C é a causa do erro da ferramenta.
