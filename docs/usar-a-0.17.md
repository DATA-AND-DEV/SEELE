# Usar o SEELE 0.17

O guia curto da versão 0.17: o que fazer, e o que esperar. O que mudou em detalhe, e o que ainda não foi medido, está nas notas
da 0.17 (empacotar/notas/0.17.0.md). As páginas marcadas HISTÓRICAS na documentação descrevem a 0.16 e não são o caminho desta
versão.

## Os endereços

| Para quê | Onde |
|---|---|
| o SEELE no navegador (e no celular) | https://seele.sh |
| o site e os downloads do app | https://get.seele.sh |
| a documentação | https://docs.seele.sh |
| o índice de MODs | https://mods.seele.sh |
| o ponto de encontro (o app e a página falam com ele sozinhos) | rendzvous.seele.sh |

O domínio antigo (seele.app.br) será desligado. Um convite antigo colado no app ainda diz o servidor e o ponto de encontro.

## Instalar

- **No computador**: baixe o app em https://get.seele.sh. Ao atualizar de uma 0.16, a 0.17 guarda os dados da 0.16 ao lado e abre a
  0.16 numa janela própria quando um link ou um servidor da 0.16 pedir.
- **No navegador**: abra https://seele.sh. No celular, é possível entrar pela aba. Instalar na Tela de Início é opcional; a identidade da aba pode ser diferente
  da usada pelo ícone, e mudar entre os dois pode exigir um novo convite.
- **Ficam no app do computador**, e não no navegador: hospedar, a portaria e os convites, instalar e ligar MODs, ligar e receber
  ligações pelo código, moderar, criar salas e canais, e abrir outras versões lado a lado.

## Entrar num servidor

1. Receba um convite (um link) de quem hospeda.
2. No app ou na página, COLAR UM LINK, conferir o servidor, escolher o apelido e ENTRAR.
3. A 0.17 fala o **Servidor 2.0** (WebRTC). Um servidor da 0.16 (Servidor 1.0) abre pela 0.16 guardada ao lado; a 0.16 não entra num
   servidor da 0.17.

## Hospedar

- No app, HOSPEDAR AQUI. O Servidor 2.0 é o padrão; o Servidor 1.0 e o 0.15 também podem ser escolhidos.
- Um servidor antigo passa pela tela ANTES DE HOSPEDAR: nada nos dados muda até o botão do fim, que faz uma cópia de segurança
  verificada, converte os dados e só então sobe o servidor.
- O dono é quem hospeda, pela chave da máquina; os cargos dizem o que cada pessoa pode fazer.

## Voz e tela

- Dá para estar em vários servidores ao mesmo tempo; o **microfone vai para um de cada vez**, o da sala de voz em que você entrou.
- Sem fone, o som do alto-falante pode voltar pelo microfone: use fone. Se o navegador só abrir o microfone sem o cancelamento de
  eco, a sala diz isso.
- Se o microfone parar durante a sala, a sala diz SEM ÁUDIO; para voltar, saia e entre de novo.
- O navegador do celular não compartilha a tela: compartilhar é pelo SEELE no computador, e a tela de quem compartilha aparece na
  chamada.

## MODs

- Um servidor 2.0 roda MODs da API 6 e da API 7. A API 7 é a da 0.17 (com as ações declaradas pelo MOD).
- **Quem hospeda decide** quais MODs o servidor tem: instalar, ligar, desligar e aplicar MODs no servidor é pelo app no computador.
- **Quem entra recebe o MOD do servidor.** No app e no navegador, a parte do MOD que roda na sua tela chega do servidor, e só depois de
  você aceitar o conjunto de MODs daquele servidor (o aceite fica guardado para aquele servidor). No navegador, os pacotes recebidos
  ficam guardados no próprio navegador.
- O navegador não instala, não liga e não gerencia MODs, nem o espaço dos pacotes: isso é do app no computador.
- Criar um MOD: o guia em docs.seele.sh/mods/. O índice está em mods.seele.sh.

## Alcance pela internet

A conversa vai direto entre as máquinas: o SEELE **não usa servidor de retransmissão (TURN)**, e o ponto de encontro só apresenta os
dois lados. Duas redes com NAT simétrico, ou uma rede que só sai por um proxy sem UDP direto, não se alcançam assim; a tela diz que o
caminho não abriu.

## Identidade

- **No computador**, a identidade é a da máquina e continua a mesma na 0.17.
- **No navegador**, tudo o que o SEELE guarda fica preso ao endereço em que foi criado. Em seele.sh você começa do zero, e **nada** do
  endereço antigo vem junto, porque o navegador separa o armazenamento por endereço:
  - a identidade (a chave) e, com ela, os cargos e os acessos;
  - a lista dos servidores em que você entrou;
  - os MODs aceitos de cada servidor e os pacotes de MOD guardados;
  - o apelido, o retrato, as prévias, e as escolhas da voz (microfone, saída, modo, sensibilidade, redução de ruído, tecla de falar).
  Não há transferência nem exportação. Quem administra cada servidor pode admitir você de novo, com um convite novo em seele.sh, e
  você refaz as escolhas.
