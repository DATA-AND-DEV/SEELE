# Medição real: SEELE com e sem MODs

**Executado em 18/09/2026, na instalação local do SEELE 0.11.2.** Teste exploratório de memória pela interface nativa, no mesmo servidor, com os três MODs completos da **API 2**. Não mede a API 3 do checkout, nem os runtimes propostos com Worker/QuickJS.

## Resultado

Valores em **MiB de footprint físico informado pelo macOS**, incluindo o aplicativo e seus processos WebKit. Cada linha resume sete amostras ao longo de aproximadamente 30 segundos. As linhas estão na ordem em que foram medidas.

| Condição | Mediana | Mínimo–máximo observado |
| --- | ---: | ---: |
| Servidor sem MODs, primeira abertura | 220,38 | 220,22–220,57 |
| Três MODs carregados, após ativação e reconexão | 253,91 | 246,14–263,96 |
| Três MODs carregados, segunda janela em repouso | 264,69 | 259,36–283,10 |
| MESA aberta, tabuleiro simples e uma peça | 287,13 | 287,04–302,38 |
| Tela inicial após sair do servidor, depois do uso dos editores | 238,58 | 238,57–238,61 |
| Aplicativo reiniciado, três MODs desde a entrada | 239,78 | 232,21–259,68 |
| Aplicativo reiniciado novamente, mesmo servidor sem MODs | 228,57 | 224,03–232,53 |

Entre as duas rodadas com processos novos, a diferença das medianas foi **11,22 MiB, aproximadamente 4,9%**. Na sequência anterior, que incluiu ativação e reconexão, a diferença entre as medianas em repouso e a primeira condição sem MODs foi de **33,53 a 44,31 MiB**. Não há um custo fixo por MOD demonstrado por esses números: o histórico da sessão, as interfaces abertas e a gestão de memória do motor influenciaram o resultado.

A condição com o tabuleiro aberto teve mediana **66,75 MiB acima da primeira condição sem MODs**. É uma carga pequena: uma campanha, um mapa sem imagem, uma peça e uma rolagem. Não representa uma campanha grande, vários participantes com GIFs, áudio, vídeo ou texturas pesadas.

## Ambiente e identificação

- Apple M5 Pro, modelo Mac17,9, **24 GiB** de RAM.
- macOS **27.0**, build **26A428**.
- Aplicativo: `/Applications/SEELE.app`, versão **0.11.2** no `Info.plist`.
- O [release v0.11.2](https://github.com/DATA-AND-DEV/SEELE-RELEASES/releases/tag/v0.11.2) declara o commit `e90bcc16751cad0861c73c3f57bfaa78c5b3e434`. Nesse commit, os clientes de MODs são carregados como scripts na janela. O SHA-256 do binário efetivamente medido está em [ambiente.json](evidencias/memoria-mods-2026-09-18/ambiente.json); não foi feita reconstrução reprodutível para comparar bytes com o commit.
- MESA **1.2.1**, PERFIS **1.2.2**, ESTILO **1.0.1**; os três manifestos instalados declaram API 2.
- Servidor separado **QA MEMORIA MODS 18-09**, criado pela UI. Um participante local, fora de sala de voz, sem chamada ou transmissão de vídeo. Nenhum convite enviado.
- Usado “Hospedar aqui”: o processo do aplicativo inclui também o servidor local. Portanto, esta medição não separa o custo de cliente e servidor dos MODs.
- Mesma janela, sem redimensionamento deliberado entre condições. Coletas sem interação durante a janela de amostragem.

## Método

Usado o utilitário nativo `/usr/bin/footprint`, com `--noCategories -f bytes`, sobre quatro processos: `seele-app`, WebKit WebContent, WebKit GPU e WebKit Networking. A tabela usa o campo **Summary Footprint** retornado pelo utilitário, convertido de bytes para MiB por divisão por 1.048.576. Não é a soma de RSS dos processos.

Os processos auxiliares foram identificados por surgirem junto com cada abertura do SEELE e desaparecerem ao encerrar o aplicativo. Um processo WebKit GPU que já existia antes do teste, PID 47113, foi excluído. As três famílias de PIDs estão no registro de ambiente. Nenhuma soma de processos WebKit de outros aplicativos entrou na coleta.

Foram coletadas **49 amostras em sete janelas**. Intervalo pretendido: cinco segundos. O script preserva timestamp, duração da chamada, saída bruta, código de retorno, footprint por processo e dados de `ps`. As duas rodadas após reiniciar incluíram espera explícita de 20 segundos antes de iniciar a coleta, depois de fechar o aviso de convite e verificar a sessão.

Para reproduzir, configurar a condição pela interface e fornecer os PIDs atuais:

```sh
python3 docs/evidencias/memoria-mods-2026-09-18/medir.py nome-da-condicao PID_APP PID_GPU PID_WEB PID_REDE
python3 docs/evidencias/memoria-mods-2026-09-18/resumir.py
```

O procedimento é exploratório, em uma máquina, com ordem sequencial e poucas repetições. Não há intervalos de confiança, controle de toda atividade do sistema ou perfil de heap. A instrumentação e a automação também podem afetar o ambiente; as coletas foram realizadas com o mesmo método.

**Não comparar diretamente com os 112 MB históricos de RSS da especificação do SEELE:** aqui há outra métrica, outra versão, servidor hospedado e processos auxiliares incluídos. Esses dados também não medem o Discord.

## O que foi verificado na interface

Os três MODs apareceram e a gestão informou código carregado. Na MESA, foram criados campanha, mapa e peça; uma rolagem foi confirmada pela UI e persistida no banco do servidor. O mapa ficou em prévia privada. Uma tentativa de arraste não alterou a posição gravada e não conta como funcionalidade validada.

O editor de ESTILO foi aberto, a paleta AURORA foi salva no servidor de teste e a sessão ficou roxa. O formulário completo de PERFIS também foi aberto, sem alterar os dados pessoais ou enviar arquivos. As medidas do tabuleiro foram feitas antes da alteração de tema; a rodada reiniciada com MODs já tinha AURORA salva, com os editores fechados. As cargas visuais não são idênticas em todas as linhas.

### Falha observada na saída

**O tema do servidor persistiu na tela inicial após sair.** A saída foi confirmada às 22:53:40; às 22:54:27, a tela inicial continuava com as cores roxas da AURORA. O servidor já não escutava na porta UDP 8383. A memória havia caído para cerca de 239 MiB, mas a restauração visual não havia sido completa.

Isso reproduz uma violação concreta do requisito de efeitos restritos à sessão **na instalação API 2 testada**. Não é uma constatação sobre a implementação Worker posterior. Também não demonstra, sozinho, um vazamento permanente de heap ou quais callbacks sobreviveram: isso exigiria instrumentação específica.

Ao encerrar o aplicativo, todos os quatro processos medidos desapareceram. A abertura final sem MODs voltou à aparência laranja original. Ao terminar o teste, o aplicativo foi encerrado e foi confirmada a ausência do servidor na porta 8383. O servidor de teste e sua campanha foram preservados para reprodução, com os três MODs desativados. Não houve alteração no código de produção nem atualização do aplicativo.

## Consequência para a proposta

O teste fornece uma referência real do que já existe: **uma experiência interativa com os três MODs pode caber, nessa carga pequena, em cerca de 240–300 MiB de footprint do conjunto aplicativo/servidor/WebKit**. Isso não autoriza prometer o mesmo consumo para qualquer conteúdo ou número de participantes.

O aumento observado não justifica, por si só, reduzir os MODs a leitura de texto. A prioridade técnica continua sendo preservar as funções e garantir a propriedade e o encerramento dos efeitos da sessão. A falha do tema mostra por que medir apenas RAM não valida a limpeza.

Os candidatos descritos em [Opções para MODs livres e leves](opcoes-mods-livres-e-leves.md) ainda precisam de um protótipo com a mesma carga para comparar custos. Este benchmark não escolhe Worker contra QuickJS e não estima o custo de WebViews adicionais.

## Evidências

- [Ambiente, versões e PIDs](evidencias/memoria-mods-2026-09-18/ambiente.json).
- [Resumo calculado das medições](evidencias/memoria-mods-2026-09-18/resumo.json).
- [Registro das ações e observações](evidencias/memoria-mods-2026-09-18/observacoes.md).
- [Script de coleta](evidencias/memoria-mods-2026-09-18/medir.py) e [cálculo do resumo](evidencias/memoria-mods-2026-09-18/resumir.py).
- Na mesma pasta, cada condição tem um JSON com as sete amostras e sete arquivos TXT com a saída original do utilitário.
