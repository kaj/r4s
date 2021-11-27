var kratsbox = function(d,w,ap) {
    function dbg() {
        if (typeof console == 'object' && console.log)
            console.log.apply(console, arguments);
    }
    if ('querySelector' in d && 'addEventListener' in w && 'forEach' in ap) {
        
        return function(selector, options) {
            var links, current, root, settings = {
                'minsize': 500,
                'next': 'next \u2192',
                'prev': '\u2190 prev',
                'close': 'close \u00D7'
            };
            for (var attrname in options) {
                settings[attrname] = options[attrname];
            }

            if (w.innerWidth < settings.minsize ||
                w.innerHeight < settings.minsize) {
                dbg('kratsbox disabled, size is %dx%d but limit is %d', w.innerWidth, w.innerHeight, settings.minsize)
                return;
            }

            function open() {
                root = d.querySelector('#kratsbox');
                if(!root) {
                    root = d.createElement('div');
                    root.id = 'kratsbox';
                    root.innerHTML = '<div><img alt="">'+
                        '<a href="#close" class="krbxbtn close">close</a>'+
                        '<a href="#next" class="krbxbtn next">next</a>'+
                        '<a href="#prev" class="krbxbtn prev">prev</a>'+
                        '<p id="krbxcaption"></p></div>';
                    d.body.appendChild(root);
                    var img = root.querySelector('img');
                    img.onload = function(e) {
                        console.log("Img loaded:", e);
                        root.classList.remove("loading");
                    }
                    function limitSize() {
                        if (img.clientWidth) {
                            var kf = root.querySelector('div');
                            kf.style.width = 'auto';
                            img.style.maxHeight = (root.clientHeight-120)+'px';
                            kf.style.width = img.clientWidth+'px';
                            ap.forEach.call(
                                kf.querySelectorAll('.extra'),
                                function(e){e.style.height=img.clientHeight+'px'})
                        } else {
                            w.setTimeout(limitSize, 100);
                        }
                    }
                    w.addEventListener('resize', limitSize);
                    img.addEventListener('load', limitSize);
                    limitSize();
                }
                
                var ce = root.querySelector('.close'),
                ne = root.querySelector('.next'),
                pe = root.querySelector('.prev');
                pe.innerHTML = settings.prev + '<span class="extra"></span>';
                ne.innerHTML = settings.next + '<span class="extra"></span>';
                ce.innerHTML = settings.close;
                ce.onclick = close;
                ne.onclick = next;
                pe.onclick = prev;
                function setupfocus(src, next, prev) {
                    src.onkeydown = function(event) {
                        if(event.which == 9) {
                            if(event.shiftKey) {
                                prev.focus();
                            } else {
                                next.focus();
                            } 
                            event.stopPropagation();
                            return false;
                        }
                    };
                }
                if (links.length > 1) {
                    root.querySelector('div').className = 'group';
                    setupfocus(ce, pe, ne);
                    setupfocus(ne, ce, pe);
                    setupfocus(pe, ne, ce);
                } else {
                    root.querySelector('div').className = 'single';
                    setupfocus(ce, ce, ce);
                }
                root.onkeydown = function(event) {
                    switch(event.which) {
                    case 9: // tab
                        ce.focus();
                        break;
                    case 27: // escape
                        close();
                        break;
                    case 37: // left arrow
                    case 38: // up arrow
                    case 33: // pgup
                    case 80: // 'p'
                        prev();
                        break;
                    case 32: // space
                    case 39: // right arrow
                    case 40: // down arrow
                    case 34: // pgdn
                    case 78: // 'n'
                        next();
                        break;
                    default:
                        return true;
                    }
                    event.stopPropagation();
                    return false;
                };
                load(this);
                root.className = 'showing';
                root.querySelector('.close').focus();
                return false;
            };
            function next() {
                return load(links[(current + 1) % links.length]);
            };
            function prev() {
                return load(links[(current+links.length-1) % links.length]);
            };
            function load(le) {
                dbg('kratsbox load', le);
                var ce = d.querySelector('#krbxcaption'),
                cap = le.getAttribute('title');
                current = parseInt(le.getAttribute('data-krbxindex'));
                root.classList.add('loading');
                root.querySelector('img').setAttribute('src', le.getAttribute('href'));
                if (cap) {
                    ce.innerHTML = cap;
                } else {
                    ce.innerHTML = ap.map.call(
                        le.parentNode.querySelectorAll('figcaption'),
                        function(e) {return e.innerHTML;}
                    ).join('<br>');
                }
                return false;
            };
            function close() {
                root.className = 'hidden';
                links[current].focus();
                return false;
            };
            
            links = d.querySelectorAll(selector);
            dbg("kratsbox selector: %s, links: %o", selector, links);
            ap.forEach.call(links, function(link, i) {
                link.setAttribute('data-krbxindex', i);
                link.onclick = open;
            });
        };
    } else {
        return function(s,o) {
            dbg("kratsbox not supported in this browser");
        }
    }
}(document,window,Array.prototype);
