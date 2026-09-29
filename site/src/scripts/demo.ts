const root = document.querySelector("[data-web-demo]");

if (root instanceof HTMLElement) {
    const apiBase = (root.dataset.apiBase || "").replace(/\/$/, "");
    const authPanel = root.querySelector("[data-auth-panel]");
    const workspace = root.querySelector("[data-workspace]");
    const alert = root.querySelector("[data-demo-alert]");
    const emailForm = root.querySelector("[data-email-form]");
    const emailStatus = root.querySelector("[data-email-status]");
    const researchForm = root.querySelector("[data-research-form]");
    const platformField = root.querySelector("[data-platform-field]");
    const platformSelect = root.querySelector("[name='platform']");
    const promptInput = root.querySelector("[name='prompt']");
    const promptLimit = root.querySelector("[data-prompt-limit]");
    const submitButton = root.querySelector("[data-run-submit]");
    const cancelButton = root.querySelector("[data-run-cancel]");
    const runOutput = root.querySelector("[data-run-output]");
    const livePanel = root.querySelector("[data-live-panel]");
    const liveFrame = root.querySelector("[data-live-frame]");
    const liveLink = root.querySelector("[data-live-link]");
    const startButton = root.querySelector("[data-run-start]");
    const activityList = root.querySelector("[data-activity]");
    const resultPanel = root.querySelector("[data-result-panel]");
    const resultElement = root.querySelector("[data-result]");
    const accountName = root.querySelector("[data-account-name]");
    const quota = root.querySelector("[data-quota]");
    let csrfToken = "";
    let currentRunId = "";
    let currentConversationId = "";
    let pollTimer = 0;
    let pollController = null;
    let pollFailures = 0;
    let lastRunWasTerminal = true;
    let stateGeneration = 0;
    let platformSelectionSupported = false;
    let promptLimits = {};

    const copy = {
        en: {
            sent: "Check your inbox for the one-time sign-in link.",
            debug: "Open the local sign-in link",
            signedOut: "You are signed out.",
            failed: "The request could not be completed. Please try again.",
            running: "Research is running…",
            cancelled: "Cancelled.",
            logoutFailed: "Sign out failed. Your server session may still be active.",
            platformMismatch: "The server did not start the selected platform. Please refresh and try again.",
            promptLimit: (value) => `Maximum ${value} characters for this platform.`,
            remaining: (value) => `${value} iterations left today`,
        },
        zh: {
            sent: "一次性登录链接已发送，请检查邮箱。",
            debug: "打开本地登录链接",
            signedOut: "已退出登录。",
            failed: "请求暂时无法完成，请稍后重试。",
            running: "正在调研…",
            cancelled: "任务已取消。",
            logoutFailed: "退出登录失败，服务端会话可能仍然有效。",
            platformMismatch: "服务端未按所选平台启动，请刷新页面后重试。",
            promptLimit: (value) => `当前平台最多输入 ${value} 个字符。`,
            remaining: (value) => `今天还可运行 ${value} 次`,
        },
    };

    const language = () =>
        document.documentElement.dataset.language === "en" ? "en" : "zh";
    const labels = () => copy[language()];
    const isTerminal = (run) =>
        ["completed", "failed", "cancelled", "create_failed"].includes(
            run.status,
        );

    const request = async (path, init = {}) => {
        const response = await fetch(`${apiBase}${path}`, {
            credentials: "include",
            ...init,
            headers: {
                ...(init.body ? { "Content-Type": "application/json" } : {}),
                ...(init.headers || {}),
            },
        });
        let body = null;
        try {
            body = await response.json();
        } catch {
            body = null;
        }
        if (!response.ok) {
            const error = new Error(body?.detail || labels().failed);
            error.status = response.status;
            throw error;
        }
        return body;
    };

    const showAlert = (message, kind = "error") => {
        if (!(alert instanceof HTMLElement)) return;
        alert.textContent = message;
        alert.dataset.kind = kind;
        alert.hidden = !message;
    };

    const selectedPlatformOption = (value) => {
        if (!(platformSelect instanceof HTMLSelectElement)) return null;
        return [...platformSelect.options].find(
            (option) => option.value === value && !option.disabled,
        );
    };

    const updatePromptLimit = () => {
        if (!(promptInput instanceof HTMLTextAreaElement)) return;
        const platform =
            platformSelect instanceof HTMLSelectElement ? platformSelect.value : "";
        const configured = Number(promptLimits[platform]);
        const limit = Number.isInteger(configured) && configured >= 10 ? configured : 3000;
        promptInput.maxLength = limit;
        if (promptLimit instanceof HTMLElement) {
            promptLimit.textContent = labels().promptLimit(limit);
        }
    };

    const loadCapabilities = async () => {
        try {
            const capabilities = await request("/v1/web/capabilities");
            const supported = new Set(
                Array.isArray(capabilities?.platforms) ? capabilities.platforms : [],
            );
            promptLimits =
                capabilities?.max_prompt_chars_by_platform &&
                typeof capabilities.max_prompt_chars_by_platform === "object"
                    ? capabilities.max_prompt_chars_by_platform
                    : {};
            if (platformSelect instanceof HTMLSelectElement) {
                platformSelectionSupported =
                    capabilities?.platform_selection === true &&
                    [...platformSelect.options].some((option) =>
                        supported.has(option.value),
                    );
                for (const option of platformSelect.options) {
                    option.disabled =
                        !platformSelectionSupported || !supported.has(option.value);
                }
                if (!selectedPlatformOption(platformSelect.value)) {
                    const fallback = [...platformSelect.options].find(
                        (option) => !option.disabled,
                    );
                    if (fallback) platformSelect.value = fallback.value;
                }
            }
        } catch {
            platformSelectionSupported = false;
            promptLimits = {};
        }
        if (platformField instanceof HTMLElement) {
            platformField.hidden = !platformSelectionSupported;
        }
        updatePromptLimit();
    };

    const stopPolling = () => {
        window.clearTimeout(pollTimer);
        pollTimer = 0;
        pollController?.abort();
        pollController = null;
        stateGeneration += 1;
    };

    const schedulePoll = (delay) => {
        window.clearTimeout(pollTimer);
        pollTimer = window.setTimeout(pollRun, delay);
    };

    const clearAccountState = () => {
        stopPolling();
        pollFailures = 0;
        currentRunId = "";
        currentConversationId = "";
        lastRunWasTerminal = true;
        if (researchForm instanceof HTMLFormElement) researchForm.reset();
        updatePromptLimit();
        if (runOutput instanceof HTMLElement) {
            runOutput.hidden = true;
            runOutput.setAttribute("aria-busy", "false");
        }
        if (livePanel instanceof HTMLElement) livePanel.hidden = true;
        if (liveFrame instanceof HTMLIFrameElement) {
            liveFrame.removeAttribute("src");
            liveFrame.tabIndex = -1;
            liveFrame.parentElement?.setAttribute("inert", "");
            liveFrame.parentElement?.removeAttribute("data-interactive");
        }
        if (liveLink instanceof HTMLAnchorElement) {
            liveLink.hidden = true;
            liveLink.removeAttribute("href");
        }
        if (startButton instanceof HTMLElement) startButton.hidden = true;
        if (activityList instanceof HTMLOListElement) activityList.replaceChildren();
        if (resultPanel instanceof HTMLElement) resultPanel.hidden = true;
        if (resultElement instanceof HTMLElement) resultElement.textContent = "";
        if (submitButton instanceof HTMLButtonElement) submitButton.disabled = false;
        if (cancelButton instanceof HTMLElement) cancelButton.hidden = true;
        if (accountName instanceof HTMLElement) accountName.textContent = "";
        if (quota instanceof HTMLElement) quota.textContent = "";
    };

    const showSignedOut = () => {
        csrfToken = "";
        clearAccountState();
        if (authPanel instanceof HTMLElement) authPanel.hidden = false;
        if (workspace instanceof HTMLElement) workspace.hidden = true;
    };

    const updateQuota = (remaining) => {
        if (quota instanceof HTMLElement) {
            quota.textContent = labels().remaining(remaining);
        }
    };

    const showSignedIn = (me) => {
        clearAccountState();
        csrfToken = me.csrf_token;
        if (authPanel instanceof HTMLElement) authPanel.hidden = true;
        if (workspace instanceof HTMLElement) workspace.hidden = false;
        if (accountName instanceof HTMLElement) {
            accountName.textContent = me.display_name || me.email || "socai user";
        }
        updateQuota(me.iterations_remaining);
        showAlert("");
    };

    const safeLiveUrl = (value) => {
        try {
            const parsed = new URL(value);
            const hostname = parsed.hostname.toLowerCase();
            const trustedHost =
                hostname === "live.browser-use.com" ||
                hostname.endsWith(".kernel.sh") ||
                hostname.endsWith(".onkernel.com");
            return parsed.protocol === "https:" && trustedHost
                ? parsed.toString()
                : "";
        } catch {
            return "";
        }
    };

    const safeDebugUrl = (value) => {
        try {
            const parsed = new URL(value);
            const localHost = ["localhost", "127.0.0.1", "::1"].includes(
                parsed.hostname,
            );
            return localHost && ["http:", "https:"].includes(parsed.protocol)
                ? parsed.toString()
                : "";
        } catch {
            return "";
        }
    };

    const renderRun = (run) => {
        currentRunId = run.id;
        currentConversationId = run.conversation_id;
        if (
            platformSelectionSupported &&
            platformSelect instanceof HTMLSelectElement &&
            selectedPlatformOption(run.platform)
        ) {
            platformSelect.value = run.platform;
            updatePromptLimit();
        }
        updateQuota(run.iterations_remaining);
        if (runOutput instanceof HTMLElement) runOutput.hidden = false;

        const liveUrl = safeLiveUrl(run.live_view_url || "");
        const interactive = Boolean(liveUrl && run.live_view_read_only === false);
        if (livePanel instanceof HTMLElement) livePanel.hidden = !liveUrl;
        if (liveFrame instanceof HTMLIFrameElement) {
            if (liveUrl && liveFrame.src !== liveUrl) liveFrame.src = liveUrl;
            if (!liveUrl) liveFrame.removeAttribute("src");
            liveFrame.tabIndex = interactive ? 0 : -1;
            liveFrame.parentElement?.toggleAttribute("inert", !interactive);
            liveFrame.parentElement?.toggleAttribute("data-interactive", interactive);
        }
        if (liveLink instanceof HTMLAnchorElement) {
            liveLink.hidden = !liveUrl;
            if (liveUrl) liveLink.href = liveUrl;
            else liveLink.removeAttribute("href");
        }
        if (startButton instanceof HTMLButtonElement) {
            startButton.hidden = !run.requires_browser_confirmation;
            startButton.disabled = false;
        }

        if (activityList instanceof HTMLOListElement) {
            activityList.replaceChildren();
            for (const item of run.activity || []) {
                const row = document.createElement("li");
                const label = document.createElement("span");
                label.textContent = item.message || item.type;
                row.append(label);
                activityList.append(row);
            }
            if (!activityList.children.length) {
                const row = document.createElement("li");
                row.textContent = labels().running;
                activityList.append(row);
            }
        }

        const terminal = isTerminal(run);
        if (runOutput instanceof HTMLElement) {
            runOutput.setAttribute("aria-busy", String(!terminal));
        }
        if (submitButton instanceof HTMLButtonElement) submitButton.disabled = !terminal;
        if (cancelButton instanceof HTMLElement) cancelButton.hidden = terminal;
        if (resultPanel instanceof HTMLElement) {
            resultPanel.hidden = !terminal;
        }
        if (resultElement instanceof HTMLElement) {
            resultElement.textContent =
                run.result || run.error || (run.status === "cancelled" ? labels().cancelled : "");
        }
        if (terminal && !lastRunWasTerminal && resultPanel instanceof HTMLElement) {
            resultPanel.focus({ preventScroll: false });
        }
        lastRunWasTerminal = terminal;
        return terminal;
    };

    const pollRun = async () => {
        if (!currentRunId) return;
        const runId = currentRunId;
        const generation = stateGeneration;
        const controller = new AbortController();
        pollController?.abort();
        pollController = controller;
        try {
            const run = await request(`/v1/web/runs/${runId}`, {
                signal: controller.signal,
            });
            if (
                controller !== pollController ||
                generation !== stateGeneration ||
                runId !== currentRunId
            ) {
                return;
            }
            pollFailures = 0;
            showAlert("");
            if (!renderRun(run)) {
                schedulePoll(2000);
            }
        } catch (error) {
            if (error.name === "AbortError" || generation !== stateGeneration) return;
            if (error.status === 401) {
                showSignedOut();
                showAlert(error.message || labels().failed);
                return;
            }
            showAlert(error.message || labels().failed);
            pollFailures += 1;
            schedulePoll(Math.min(15000, 1000 * 2 ** pollFailures));
        } finally {
            if (pollController === controller) pollController = null;
        }
    };

    const restoreLatestRun = async () => {
        const generation = stateGeneration;
        if (submitButton instanceof HTMLButtonElement) submitButton.disabled = true;
        try {
            const runs = await request("/v1/web/runs");
            if (generation !== stateGeneration) return;
            pollFailures = 0;
            const run =
                runs.find((candidate) => !isTerminal(candidate)) || runs[0];
            if (!run) {
                if (submitButton instanceof HTMLButtonElement) {
                    submitButton.disabled = false;
                }
                return;
            }
            lastRunWasTerminal = isTerminal(run);
            if (!renderRun(run)) {
                schedulePoll(1200);
            }
        } catch (error) {
            if (generation !== stateGeneration) return;
            if (error.status === 401) {
                showSignedOut();
                return;
            }
            showAlert(error.message || labels().failed);
            pollFailures += 1;
            window.clearTimeout(pollTimer);
            pollTimer = window.setTimeout(
                restoreLatestRun,
                Math.min(15000, 1000 * 2 ** pollFailures),
            );
        }
    };

    const loadMe = async () => {
        try {
            showSignedIn(await request("/v1/web/auth/me"));
            await restoreLatestRun();
        } catch (error) {
            if (error.status === 401) {
                showSignedOut();
                return;
            }
            showAlert(error.message || labels().failed);
        }
    };

    emailForm?.addEventListener("submit", async (event) => {
        event.preventDefault();
        const data = new FormData(emailForm);
        try {
            const response = await request("/v1/web/auth/email/start", {
                method: "POST",
                body: JSON.stringify({ email: data.get("email") }),
            });
            if (emailStatus instanceof HTMLElement) {
                emailStatus.replaceChildren(document.createTextNode(labels().sent));
                const debugUrl = safeDebugUrl(response.debug_url || "");
                if (debugUrl) {
                    const link = document.createElement("a");
                    link.href = debugUrl;
                    link.textContent = labels().debug;
                    emailStatus.append(document.createElement("br"), link);
                }
            }
        } catch (error) {
            if (emailStatus instanceof HTMLElement) {
                emailStatus.textContent = error.message || labels().failed;
            }
        }
    });

    researchForm?.addEventListener("submit", async (event) => {
        event.preventDefault();
        stopPolling();
        const generation = stateGeneration;
        const data = new FormData(researchForm);
        if (submitButton instanceof HTMLButtonElement) submitButton.disabled = true;
        showAlert("");
        try {
            const requestedPlatform = platformSelectionSupported
                ? String(data.get("platform") || "")
                : "";
            const body = { prompt: data.get("prompt") };
            if (requestedPlatform) body.platform = requestedPlatform;
            if (currentConversationId) body.conversation_id = currentConversationId;
            const run = await request("/v1/web/runs", {
                method: "POST",
                headers: { "X-CSRF-Token": csrfToken },
                body: JSON.stringify(body),
            });
            if (generation !== stateGeneration) return;
            if (requestedPlatform && run.platform !== requestedPlatform) {
                try {
                    await request(`/v1/web/runs/${run.id}/cancel`, {
                        method: "POST",
                        headers: { "X-CSRF-Token": csrfToken },
                    });
                } catch {
                    // The mismatch remains visible even if best-effort cleanup fails.
                }
                throw new Error(labels().platformMismatch);
            }
            lastRunWasTerminal = false;
            renderRun(run);
            schedulePoll(1200);
        } catch (error) {
            if (generation !== stateGeneration) return;
            if (submitButton instanceof HTMLButtonElement) submitButton.disabled = false;
            showAlert(error.message || labels().failed);
        }
    });

    platformSelect?.addEventListener("change", updatePromptLimit);
    document.querySelectorAll("[data-lang-option]").forEach((option) => {
        option.addEventListener("click", () => queueMicrotask(updatePromptLimit));
    });

    cancelButton?.addEventListener("click", async () => {
        if (!currentRunId) return;
        stopPolling();
        const generation = stateGeneration;
        try {
            const runId = currentRunId;
            const run = await request(`/v1/web/runs/${runId}/cancel`, {
                method: "POST",
                headers: { "X-CSRF-Token": csrfToken },
            });
            if (generation !== stateGeneration || runId !== currentRunId) return;
            renderRun(run);
        } catch (error) {
            if (generation !== stateGeneration) return;
            showAlert(error.message || labels().failed);
            schedulePoll(1500);
        }
    });

    startButton?.addEventListener("click", async () => {
        if (!currentRunId || !(startButton instanceof HTMLButtonElement)) return;
        stopPolling();
        const generation = stateGeneration;
        const runId = currentRunId;
        startButton.disabled = true;
        try {
            const run = await request(`/v1/web/runs/${runId}/start`, {
                method: "POST",
                headers: { "X-CSRF-Token": csrfToken },
            });
            if (generation !== stateGeneration || runId !== currentRunId) return;
            renderRun(run);
            schedulePoll(800);
        } catch (error) {
            if (generation !== stateGeneration) return;
            startButton.disabled = false;
            showAlert(error.message || labels().failed);
            schedulePoll(1500);
        }
    });

    root.querySelector("[data-sign-out]")?.addEventListener("click", async () => {
        stopPolling();
        try {
            await request("/v1/web/auth/logout", {
                method: "POST",
                headers: { "X-CSRF-Token": csrfToken },
            });
            showSignedOut();
            showAlert(labels().signedOut, "success");
        } catch {
            clearAccountState();
            await loadMe();
            showAlert(labels().logoutFailed);
        }
    });

    request("/v1/web/auth/providers")
        .then((providers) => {
            root.querySelectorAll("[data-provider]").forEach((element) => {
                const provider = element.getAttribute("data-provider");
                if (element instanceof HTMLElement && provider) {
                    element.hidden = !providers[provider];
                }
            });
        })
        .catch(() => {});

    const query = new URLSearchParams(window.location.search);
    if (query.get("auth_error")) showAlert(labels().failed);
    if (query.has("auth") || query.has("auth_error")) {
        window.history.replaceState({}, "", window.location.pathname);
    }
    const initialize = async () => {
        await loadCapabilities();
        await loadMe();
    };
    initialize();
}
