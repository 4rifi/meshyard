import type { MemberView } from '../../ipc/generated/MemberView';
export function MemberList({ members }: { members: MemberView[] }) {
  return (
    <ul className="items">
      {members.map((member) => (
        <li key={member.device_short}>
          <strong>{member.display_name || 'Unnamed member'}</strong>
          <span>
            {member.role === 'Owner' ? 'Owner' : 'Member'}
            {member.is_you ? ' · You' : ''}
          </span>
          <code>{member.device_short}</code>
        </li>
      ))}
    </ul>
  );
}
