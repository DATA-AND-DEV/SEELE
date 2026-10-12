"""O operador.barra do desktop sob uma região clara: o contraste de cada texto
da `.operador`, sem tema e com a região, pela bancada do celular sem a marca.

Rodar, da raiz do repositório:  python3 docs/evidencias/f8-regiao-do-operador/medir.py <pasta-de-saída>
"""
import importlib.util, pathlib, shutil, sys

raiz = pathlib.Path(__file__).resolve().parents[3]
saida = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("bancada", raiz / "tools" / "bancada-do-celular.py")
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
shutil.rmtree(saida, ignore_errors=True)
saida.mkdir(parents=True)

from playwright.sync_api import sync_playwright

with sync_playwright() as pw:
    nav = pw.chromium.launch()
    pagina = b.montar(saida, celular=True, marca=None)
    pg = nav.new_page(viewport={"width": 1400, "height": 900})
    pg.goto(f"file://{pagina}")
    b.entrar(pg)
    pg.evaluate(b.MOD_DE_PE)
    pg.evaluate("__pedirAoProduto({ tipo: 'contribuir', pedido: { ponto: 'operador.barra', modo: 'adicionar', conteudo: [{ forma: 'texto', chave: 's', dentro: 'SONDA DA REGIÃO' }] } })")
    pg.wait_for_timeout(800)
    sem = pg.evaluate(b.CONTRASTE_EM, ["#tela-sessao .operador", None])
    # A região no quadro, como o servidor a manda: o redesenho a mantém.
    pg.evaluate("(cores) => { SEELE_QUADRO.tema_do_servidor = { mod_id: 'seele/estilo', valores: JSON.stringify({ regioes: { 'operador.barra': cores } }) }; }", b.REGIAO_CLARA)
    pg.wait_for_timeout(1500)
    com = pg.evaluate(b.CONTRASTE_EM, ["#tela-sessao .operador", None])
    for texto, antes in sem.items():
        print(f"{texto}: {antes} → {com.get(texto)}")
    pg.screenshot(path=str(saida / "desktop.png"))
    nav.close()
