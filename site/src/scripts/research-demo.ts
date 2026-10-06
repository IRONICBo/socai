import { getDemoCopy, demoScenarioCount } from "./research-demo-copy";

// One visible-time clock drives the browser choreography and loops after the
// completed report. Background tabs and offscreen demos do not consume frames.
let controller: { setLanguage: (language: string) => void } | undefined;

export function startResearchDemo(language: string) {
    const root = document.querySelector<HTMLElement>("[data-research-demo]");
    if (!root) return;
    if (!controller) controller = createDemo(root);
    controller.setLanguage(language);
}

function createDemo(root: HTMLElement) {
    const get = (selector: string) => root.querySelector<HTMLElement>(selector)!;
    const typing = get("[data-demo-typing]");
    const summaries = Array.from(root.querySelectorAll<HTMLElement>("[data-demo-summary]"));
    const status = get("[data-demo-status]");
    const composer = get(".demo-compose");
    const workspace = get(".demo-workspace");
    const result = get(".demo-result");
    const pointer = get(".demo-pointer");
    const posts = Array.from(root.querySelectorAll<HTMLElement>("[data-demo-post]"));
    const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
    const duration = 40000;
    let language = "en";
    let scenario = 0;
    let copy = getDemoCopy(language, scenario);
    let elapsed = reducedMotion.matches ? 32000 : 0;
    let inView = false;
    let frame = 0;
    let previous = 0;
    const progress = (start: number, end: number) => Math.min(1, Math.max(0, (elapsed - start) / (end - start)));

    const render = () => {
        const stage = elapsed < 3400 ? "typing" : elapsed < 4300 ? "opening"
            : elapsed < 6300 ? "searching" : elapsed < 25000 ? "reading"
            : elapsed < 30000 ? "writing" : "complete";
        const platform = elapsed < 12000 ? "instagram" : elapsed < 18500 ? "x" : "tiktok";
        root.dataset.scenario = String(scenario);
        const detail = elapsed >= 8500 && elapsed < 11700;
        root.dataset.stage = stage;
        root.dataset.platform = platform;
        const profile = elapsed >= 22800 && elapsed < 25000;
        root.dataset.view = detail ? "post" : profile ? "profile" : "search";
        get("[data-demo-profile]").setAttribute("aria-hidden", String(!profile));
        root.dataset.paused = String(!inView || document.hidden);
        root.style.setProperty("--demo-progress", `${elapsed / duration * 100}%`);
        typing.textContent = Array.from(copy.question).slice(0, Math.floor(progress(350, 3050) * Array.from(copy.question).length)).join("");
        composer.inert = elapsed >= 4300;
        composer.setAttribute("aria-hidden", String(elapsed >= 4300));
        workspace.inert = elapsed < 4300;
        workspace.setAttribute("aria-hidden", String(elapsed < 4300));
        const statusKey = stage === "searching" ? "instagramSearch" : stage === "reading"
            ? (elapsed < 8500 ? "instagramSearch" : elapsed < 12000 ? "instagramRead" : elapsed < 14300 ? "xSearch" : elapsed < 18500 ? "xRead" : elapsed < 20800 ? "tiktokSearch" : "tiktokRead") : stage;
        if (status.textContent !== copy[statusKey]) status.textContent = copy[statusKey];
        const instagramCount = Math.round(progress(6300, 11700) * 72);
        const xCount = Math.round(progress(14300, 18500) * 56);
        const tiktokCount = Math.round(progress(20800, 25000) * 40);
        get("[data-demo-total]").textContent = `${instagramCount + xCount + tiktokCount} ${copy.sourceUnit}`;
        posts.forEach((post) => {
            const visible = elapsed >= Number(post.dataset.at);
            post.classList.toggle("is-visible", visible);
            post.setAttribute("aria-hidden", String(!visible));
        });
        for (const name of ["instagram", "x", "tiktok"] as const) {
            const panel = get(`.demo-feed--${name}`);
            panel.inert = name !== platform;
            panel.setAttribute("aria-hidden", String(name !== platform));
            get(`[data-demo-count="${name}"]`).textContent = `${name === "instagram" ? instagramCount : name === "x" ? xCount : tiktokCount} ${copy.posts}`;
            const query = copy[name === "instagram" ? "queryInstagram" : name === "x" ? "queryX" : "queryTiktok"];
            const start = name === "instagram" ? 4600 : name === "x" ? 12300 : 18800;
            get(`[data-demo-query="${name}"]`).textContent = query.slice(0, Math.floor(progress(start, start + 1100) * query.length));
        }
        get("[data-demo-address]").textContent = platform === "instagram"
            ? detail ? `instagram.com/p/${scenario === 0 ? "daily-texture" : "running-wear-test"}/` : "instagram.com/explore/search/keyword/"
            : platform === "x" ? `x.com/search?q=${encodeURIComponent(copy.queryX)}&f=live`
            : profile ? `tiktok.com/${copy.tkName1}` : `tiktok.com/search?q=${encodeURIComponent(copy.queryTiktok)}`;
        const xFeed = get(".demo-posts--x");
        xFeed.style.transform = `translateY(-${Math.max(0, xFeed.scrollHeight - xFeed.parentElement!.clientHeight + 12) * progress(15200, 18300)}px)`;
        result.classList.toggle("is-visible", elapsed >= 25000);
        result.setAttribute("aria-hidden", String(elapsed < 25000));
        const reportLength = summaries.reduce((total, element) => total + Array.from(copy[`report${element.dataset.demoSummary}`]).length, 0);
        let remaining = Math.ceil(progress(25000, 30000) * reportLength);
        summaries.forEach((element) => {
            const chars = Array.from(copy[`report${element.dataset.demoSummary}`]);
            element.textContent = chars.slice(0, Math.max(0, remaining)).join("");
            element.parentElement!.classList.toggle("is-written", remaining > 0);
            remaining -= chars.length;
        });
        root.style.setProperty("--video-progress", `${progress(21400, 24600) * 100}%`);

        // The pointer visits actual rendered UI targets, so clicks stay aligned
        // with the search field, result and close icon at every viewport width.
        const target = elapsed >= 18200 && elapsed < 18800 ? '[data-browser-tab="tiktok"]'
            : elapsed >= 11700 && elapsed < 12300 ? '[data-browser-tab="x"]'
            : elapsed < 7300 ? ".demo-feed--instagram .demo-search"
            : elapsed < 10500 ? ".demo-instagram:first-child .demo-art"
            : elapsed < 12000 ? ".demo-feed--instagram .demo-close-post"
            : elapsed < 14300 ? ".demo-feed--x .demo-search"
            : elapsed < 18500 ? ".demo-tweet:nth-child(2)"
            : elapsed < 21000 ? ".demo-feed--tiktok .demo-search"
            : elapsed < 22800 ? ".demo-video:first-child .demo-video__screen" : ".demo-video:first-child .demo-video__handle";
        const bounds = get(target).getBoundingClientRect();
        const page = get(".demo-browser__page").getBoundingClientRect();
        pointer.style.left = `${Math.min(page.width - 88, Math.max(8, bounds.left - page.left + bounds.width * .65))}px`;
        pointer.style.top = `${Math.min(page.height - 32, Math.max(-65, bounds.top - page.top + Math.min(bounds.height * .5, 70)))}px`;
        pointer.classList.toggle("is-clicking", [4600, 8200, 11400, 12000, 12700, 18500, 19300, 21400, 22800].some(time => elapsed >= time && elapsed < time + 450));
    };
    const canRun = () => inView && !document.hidden && !reducedMotion.matches;
    const tick = (now: number) => {
        frame = 0;
        if (!canRun()) { previous = 0; return; }
        if (previous) {
            elapsed += Math.min(now - previous, 100);
            if (elapsed >= duration) {
                elapsed %= duration;
                scenario = (scenario + 1) % demoScenarioCount;
                applyCopy();
            }
        }
        previous = now;
        render();
        if (canRun()) frame = requestAnimationFrame(tick);
    };
    const sync = () => {
        previous = 0;
        if (frame) cancelAnimationFrame(frame);
        frame = 0;
        render();
        if (canRun()) frame = requestAnimationFrame(tick);
    };
    document.addEventListener("visibilitychange", sync);
    reducedMotion.addEventListener("change", () => {
        elapsed = reducedMotion.matches ? 32000 : 0;
        sync();
    });
    new IntersectionObserver(([entry]) => {
        inView = entry.isIntersecting;
        sync();
    }, { threshold: 0.15 }).observe(root);
    new ResizeObserver(render).observe(get(".demo-browser__page"));
    function applyCopy() {
        copy = getDemoCopy(language, scenario);
        root.querySelectorAll<HTMLElement>("[data-demo-copy]").forEach((element) => {
            element.textContent = copy[element.dataset.demoCopy!] || "";
        });
        root.setAttribute("aria-label", copy.region);
    }
    return { setLanguage(nextLanguage: string) {
        language = nextLanguage;
        applyCopy();
        sync();
    } };
}
