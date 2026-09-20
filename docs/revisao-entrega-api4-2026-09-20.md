# Revisão da entrega API 4 — checkout 7ca66cc

Data: 20/09/2026. Revisão do relato de conclusão da v0.13.0, do código e dos registros locais. **Conclusão: há implementação substancial, mas este checkout ainda não está pronto para publicação estável.** A ausência de homologação não é o único motivo: há defeitos reproduzidos e capacidades declaradas sem integração.

A consulta ao release público nesta revisão ainda retornou v0.12.1, commit de origem `655a137`. Nenhuma publicação foi feita. Esta revisão não reexecutou as suítes alegadas no relato e não testou o candidato no aplicativo nativo; distingue inspeção e reproduções isoladas abaixo.

## Evidência executada

[Reprodução pontual](evidencias/revisao-api4-7ca66cc.cjs), usando os métodos reais do checkout com dependências mínimas simuladas. Executar da raiz:

```sh
node docs/evidencias/revisao-api4-7ca66cc.cjs
```

Resultado observado:

```text
modal: ancestral tela-sessao marcado inert=true
fechar -> mostrar: visivel=true, mas nó segue removido
preferência vazia: seleciona mod/a, não restaura apresentação nativa
1000 registrar/revogar: contribuições vivas=0; descartadores retidos=1000
mod/b revoga contribuição de mod/a: aceito pelo roteador
mensagem direta API 3 -> contribuir (API 4): aceita pelo roteador
```

O script registra os defeitos deste checkout; suas asserções descrevem o estado defeituoso e não devem ser copiadas como critérios de sucesso da implementação. Ao corrigir, criar/inverter as verificações correspondentes. Não é navegador, Tauri nem medição de memória: o número 1000 é contagem de referências retidas, não bytes de RAM.

## R1 — P1: o modal torna seu próprio ancestral inerte

Em [mods-superficies.js](../apps/seele-app/ui/mods-superficies.js), `prenderFoco` (linhas 191–200) percorre os filhos de `document.body` e excetua apenas o próprio `palcos.camadas`.

No [index.html do aplicativo](../apps/seele-app/ui/index.html), esse palco está **dentro de `section#tela-sessao`** (linha 1733). O parser do HTML confirmou a ancestralidade `html > body > section#tela-sessao`. Assim, o código marca a sessão como inerte, incluindo o diálogo descendente. A consequência esperada no navegador é impedir interação/foco nos controles do próprio modal; a atribuição incorreta foi reproduzida com o método real.

O [preview de PERFIS](../../SEELE-MOD-PERFIS/ferramentas/preview.cjs) coloca o palco **diretamente no body** (linha 200). Usar os mesmos arquivos de renderer num HTML diferente não reproduz essa integração. O teste de UI abre a primeira superfície e conta botão/origem; não executa a edição completa do modal no HTML real.

**Correção e aceite:** coordenador de camadas que preserve o ramo ativo e o estado inerte anterior; verificar modal inicial, confirmação de descarte e modal filho no aplicativo. A confirmação nativa também precisa ficar interativa e acima da camada do MOD. Abrir, digitar, Tab/Shift+Tab, cancelar e fechar devem funcionar com retorno de foco e sem deixar a sessão bloqueada.

## R2 — P1: a revogação não confere o dono da contribuição

Em [base.js](../apps/seele-app/ui/base.js), linha 1525, qualquer instância ativa envia um handle a `contribuicoesDosMods.revogar(m.handle)`. O registro conhece dono/instância/geração, mas o método recebe apenas o handle e não os confere. Os handles são sequenciais.

A reprodução passou uma contribuição de A ao roteador real como pedido de B: a contribuição foi removida e B recebeu sucesso. Não é fuga do QuickJS nem acesso ao sistema; é interferência entre MODs pela API do produto.

**Correção e aceite:** conferir instância/geração/dono no host antes de revogar. Separar operação interna privilegiada de limpeza da operação pública. B não remove A; A remove a própria contribuição; a saída continua limpando ambas.

Ainda no roteador, um pedido direto de pacote declarado API 3 para `contribuir` é aceito. O prelúdio omite o método, mas `seele.postar` permite emitir a mensagem diretamente. **Capacidades por versão devem ser conferidas no host**, usando a versão confiável do pacote/instância, não um campo enviado pelo MOD.

## R3 — P1: oito pontos são aceitos, mas não aplicados na interface

O registro em [mods-contribuicoes.js](../apps/seele-app/ui/mods-contribuicoes.js), linhas 52–63, anuncia dez pontos. Os consumidores de apresentação encontrados no código de UI são `pessoa.cartao` e `servidor.navegacao`.

Não há aplicação de contribuições registradas para:

- `pessoa.identidade`;
- `pessoa.detalhes`;
- `pessoa.acoes`;
- `canal.item`;
- `canal.cabecalho`;
- `compositor.ferramentas`;
- `sala.acoes`;
- `servidor.aparencia`.

Listá-los na gestão ou retornar um handle não os implementa. O caminho `cartaoDeContribuicao` também consulta `SeeleUI.cartoes` legado; não monta o `conteudo` da contribuição como sugere o contrato genérico.

**Correção e aceite:** integrar os pontos prometidos aos componentes reais, incluindo modos aceitos e ações. Cada exemplo deve produzir uma alteração visível e voltar ao padrão ao revogar. Enquanto um ponto estiver incompleto, não anunciá-lo como capacidade nem aceitar silenciosamente seu registro. O objetivo solicitado continua sendo completar a integração, não apenas retirar nomes da documentação.

## R4 — P1: descarte explícito acumula recursos durante a sessão

`registrar` em [mods-contribuicoes.js](../apps/seele-app/ui/mods-contribuicoes.js), linhas 158–160, adiciona um descartador à instância. `revogar` libera a entrada/cota, mas não retira esse descartador de `InstanciaDeMod.recursos`. A closure continua retendo a contribuição e seu conteúdo até encerrar a instância.

A reprodução com as classes reais terminou com **zero contribuições vivas e 1000 descartadores retidos**. Logo, o teto de 128 contribuições simultâneas não limita o histórico retido. O mesmo padrão merece conferência no descarte de superfícies.

**Correção e aceite:** registro de recursos com liberação/removal idempotente, que remova a referência quando o recurso já foi descartado. Repetir criar/revogar e criar/descartar deve estabilizar contagem e memória viva durante uma sessão longa; saída ainda precisa funcionar. Não há necessidade de inventar novo teto antes de corrigir a retenção.

## R5 — P1: “Usar apresentação padrão” não representa o padrão nativo

Em [camada-mods.js](../apps/seele-app/ui/camada-mods.js), linhas 586–589, escolher ID vazio apaga a preferência. Em `escolherSubstituicao`, preferência ausente escolhe a primeira candidata. Portanto, o comando anunciado como recuperação retorna à seleção automática de MODs, não à apresentação nativa.

**Correção e aceite:** distinguir três estados: automático, provedor específico e nativo explícito. A seleção nativa deve ignorar substituições, sobreviver a redesenhos e ser aplicada apenas ao escopo/servidor desejado. A chave de armazenamento atual também é global por ponto; o registro de decisões a descreve como “por destino”, mas não há destino na chave nem no mapa.

## R6 — P2: visibilidade e fechamento têm contratos incompatíveis

Em [mods-superficies.js](../apps/seele-app/ui/mods-superficies.js), `fechar` remove o nó do palco; `mostrar` só altera flags/hidden e não o reinsere. A sequência real de métodos termina com superfície dita visível, ainda fora do documento. `criar` novamente com o mesmo ID chama `abrir`, mas isso não torna o método público `mostrar` correto.

Além disso, `ocultar` de um modal não libera o foco nem o estado inerte da aplicação. A implementação deve coordenar visibilidade, montagem, mídia, foco e palcos, inclusive em fechamento fora da ordem entre dois diálogos.

**Correção e aceite:** estados explícitos e transições idempotentes. Criar → ocultar → mostrar → fechar → mostrar → descartar deve ter comportamento documentado e exercitado; não retornar sucesso numa superfície desconectada.

## R7 — P1 para concluir a entrega: o guia local não acompanha a API 4

O [guia-fonte do site](../../SEELE-MODS-INDEXER/site/guia/guia-criacao-mods.md) continua ensinando API 3, afirmando igualdade exata de versão (linha 310) e descrevendo recursos suspensos dos MODs 2.0.0. O HTML gerado repete essas orientações. Manter um exemplo API 3 como compatibilidade é válido; apresentá-lo como referência única da API atual não é.

O arquivo `api/v4.json` inventaria capacidades, mas não substitui referência de propriedades, métodos, eventos, resultados, falhas e exemplos executáveis para quem cria MODs.

**Correção e aceite:** atualizar guia-fonte, site gerado, exemplos e instruções de migração, com a matriz real de versões. Um exemplo para cada ponto de integração ajuda também a detectar R3. Não publicar o guia como API estável antes de os contratos corresponderem ao comportamento.

## O que o relato pode afirmar

Há mudanças reais: correções dos pedidos MESA, controles de arquivo, novas estruturas de UI, reorganização das configurações, gramática visual e compatibilidade de manifesto. Isso justifica chamar a entrega de **candidato em validação**. Não justifica “todos os 33 achados concluídos” nem “API pronta para publicar” enquanto os defeitos acima persistem.

A configuração passou a uma camada ampla com cabeçalho e agrupamento, melhoria coerente com parte da auditoria. O painel rápido de áudio foi explicitamente omitido: registrar essa decisão é correto, mas não equivale a entregar a combinação recomendada.

Também há redução de escopo não completamente inventariada: o plano propunha outras superfícies, estilos/fontes e keyframes, componentes e contratos de estado. A entrega precisa de uma matriz “implementado / parcial / pendente”, e não de um total de nomes novos. Não é necessário fabricar funcionalidades para preencher uma lista; é necessário tornar o escopo acordado verificável.

## Próximo ciclo, com fim definido

1. Corrigir R1–R6 e converter as reproduções pertinentes em testes do comportamento correto, cobrindo o HTML e o roteador reais.
2. Completar as integrações/guia de R3 e R7, registrando explicitamente quaisquer decisões de escopo ainda abertas.
3. Executar o candidato nativo com os três pacotes candidatos: criar campanha, editar/salvar perfil, prévia/publicação do tema, abrir/fechar/reabrir superfícies, restaurar apresentação nativa e sair.
4. Medir o app inteiro sem MOD, com três ativos, com uma UI aberta e após ciclos de saída. Complementar com participante/voz e plataformas declaradas para a release.

**Não é correto usar “precisa de duas máquinas” para adiar todas as jornadas.** Criar campanha, editar perfil, navegar, confirmar descarte, repetir abertura/fechamento e verificar saída já podem ser feitos com um cliente. Os trechos de sincronização, autorização de outro participante e qualidade de voz ficam identificados separadamente.

Não executar outra bateria genérica só para obter um número maior de testes. A condição de término é corrigir os defeitos reproduzidos e observar os fluxos prometidos no produto integrado, com resultados e pendências proporcionais ao escopo da release.

