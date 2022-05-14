let d = document.documentElement;
let t = localStorage.getItem('theme');
if (t) {
    d.classList.add('theme-' + t)
}
function init() {
    let h = d.querySelector('header');
    if (d.lang == 'sv') {
        theme = 'tema'
    } else {
        theme = 'theme'
    }
    h.insertAdjacentHTML(
        'beforeend',
        `<p><button class="themeswitch">${theme}</button></p>`
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
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
