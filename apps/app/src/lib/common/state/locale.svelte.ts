import {
    getLocale as getParaglideLocale,
    type Locale,
    setLocale as setParaglideLocale,
} from "$lib/paraglide/runtime";

let locale = $state<Locale>(getParaglideLocale());

export async function setLocale(newLocale: Locale) {
    locale = newLocale;
    setParaglideLocale(newLocale);
}

export function getLocale() {
    return locale;
}
