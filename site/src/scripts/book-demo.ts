const root = document.querySelector("[data-book-demo]");

if (root instanceof HTMLElement) {
    const apiBase = (root.dataset.apiBase || "").replace(/\/$/, "");
    const form = root.querySelector("[data-book-form]");
    const status = root.querySelector("[data-book-status]");
    const messages = {
        zh: { success: "预约已收到，我们会通过邮箱联系你。", failure: "提交失败，请稍后重试。" },
        en: { success: "Request received. We will follow up by email.", failure: "The request failed. Please try again." },
    };

    form?.addEventListener("submit", async (event) => {
        event.preventDefault();
        const language = document.documentElement.dataset.language === "en" ? "en" : "zh";
        const data = new FormData(form);
        const button = form.querySelector("button[type='submit']");
        if (button instanceof HTMLButtonElement) button.disabled = true;
        try {
            const response = await fetch(`${apiBase}/v1/web/demo-requests`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    name: data.get("name"),
                    email: data.get("email"),
                    company: data.get("company"),
                    use_case: data.get("use_case"),
                    website: data.get("website"),
                    source: "website-book-demo",
                }),
            });
            if (!response.ok) throw new Error("request failed");
            if (status instanceof HTMLElement) {
                status.textContent = messages[language].success;
                status.dataset.kind = "success";
                status.hidden = false;
            }
            form.reset();
        } catch {
            if (status instanceof HTMLElement) {
                status.textContent = messages[language].failure;
                status.dataset.kind = "error";
                status.hidden = false;
            }
        } finally {
            if (button instanceof HTMLButtonElement) button.disabled = false;
        }
    });
}
