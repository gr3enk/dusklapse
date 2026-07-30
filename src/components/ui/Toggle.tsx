export default function Toggle({
    checked,
    onChange,
    disabled = false,
}: {
    checked: boolean;
    onChange: (checked: boolean) => void;
    disabled?: boolean;
}) {
    return (
        <label className="switch">
            <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} disabled={disabled} />
            <span className="slider round"></span>
        </label>
    );
}
