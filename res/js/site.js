let d = document.documentElement;
let t = localStorage.getItem('theme');
if (t) {
    d.classList.add('theme-' + t)
}
function init() {
    let _ = d.lang == 'sv'? (e, s) => s: (e, s) => e;
    let l = {
        theme: _('theme', 'tema'),
        fedi: _('Share on fediverse', 'Dela i fediversum'),
        f_info: _(
            'Enter your instance and <i>Share</i>.  A window will open where you can edit and send your toot.',
            'Ange din instans och <i>Dela</i>, så öppnas ett fönster där du kan redigera och skicka din toot.'),
        f_byme: _('by @rkaj@mastodon.nu', 'av @rkaj@mastodon.nu'),
        f_ins: _('Instance', 'Instans'),
        f_canc: _('Cancel', 'Avbryt'),
        f_ok: _('Share', 'Dela'),
        f_insval: _(
            'Use the fully qualified hostname of your instance (no slashes).',
            'Använd det fullständiga hostnamnet för din instans (inga snedstreck).')
    };

    let h = d.querySelector('header');
    h.insertAdjacentHTML(
        'beforeend',
        `<p><button class="themeswitch">${l.theme}</button></p>`
    );
    h.querySelector('button.themeswitch').addEventListener(
        'click',
        function(e) {
            let c = d.classList;
            let l = localStorage;
            if (c.replace('theme-dark', 'theme-light')) {
                l.setItem('theme', 'light')
            } else if (c.replace('theme-light', 'theme-default')) {
                l.removeItem('theme')
            } else {
                c.remove('theme-default');
                c.add('theme-dark');
                l.setItem('theme', 'dark')
            }
            e.target.blur();
        }
    );
    let s = document.querySelector('menu.social');
    if (s) {
        s.insertAdjacentHTML(
            'afterbegin',
            `<li class="fedishare"><button class="open">${l.fedi}</button>
<dialog><form method="dialog">
<h3>${l.fedi}</h3>
<p>${l.f_info}</p>
<label>${l.f_ins}: <input name="instance" required pattern="^[\\w\\.]+\\.\\w+$"></label>
<div><button type="reset" value="cancel">${l.f_canc}</button>
<button value="share">${l.f_ok}</button></div>
</form></dialog>
</li>`);
        let f = s.querySelector('li.fedishare');
        let d = f.querySelector('dialog');
        f.querySelector('button.open').addEventListener('click', (e) => d.showModal());
        d.querySelector('button[type=reset]').addEventListener('click', (e) => d.close());
        d.querySelector('button[value=share]').addEventListener(
            'click',
            (e) => {
                let i_e = d.querySelector('input[name=instance]');
                i_e.setCustomValidity('');
                if (i_e.checkValidity()) {
                    let h1 = document.querySelector('h1').textContent;
                    let ts = [...document.querySelectorAll('p.publine a[rel=tag]')]
                        .map((e) => `#${e.text.replace(/\s+/, '-')}`).join(' ');
                    let u = new URL('https://f/share');
                    u.host = i_e.value;
                    u.search = new URLSearchParams({
                        text: `${h1}, ${l.f_byme}\n\n${ts}\n`,
                        url: window.location,
                    });
                    window.open(u, "r4share", "popup,noopener,height=600,width=450");
                } else {
                    console.log(`The value "${i_e.value}" is invalid`);
                    i_e.setCustomValidity(l.f_insval);
                }
            }
        );
    }
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
