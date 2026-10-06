import type { ButtonHTMLAttributes } from 'react';
import { Icon, type IconName } from './Icon';

type Props = ButtonHTMLAttributes<HTMLButtonElement> & { icon?: IconName; variant?: 'primary' | 'secondary' | 'quiet'; };

export function ActionButton({ icon, variant = 'secondary', className = '', children, type = 'button', ...props }: Props) {
  return <button type={type} className={`action-button ${variant === 'primary' ? 'primary' : variant === 'quiet' ? 'quiet' : ''} ${className}`} {...props}>{icon && <span className="button-symbol"><Icon name={icon} size={15} /></span>}<span className="button-label">{children}</span></button>;
}
