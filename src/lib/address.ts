/**
 * Reading an address the way it was meant, not the way the keyboard allowed.
 *
 * A camera on its own access point is reached by IPv4 and nothing else, so the address field asks
 * iOS for the numeric keypad. That keypad carries the *locale's* decimal separator and no other
 * punctuation: on a German iPhone the only key next to the digits is a comma, and an IPv4 address
 * cannot be typed at all. An iPad hides the problem, because its keypad is a full keyboard.
 *
 * The alternative is a QWERTY keyboard for four numbers, which costs everyone a layer switch to
 * fix something only some devices have. Taking the comma to mean what the person obviously meant
 * by it is cheaper and works on both.
 */
export function normalizeAddress(typed: string): string {
    return typed.replace(/,/g, ".").trim();
}
