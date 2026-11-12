// Census page: convoys. Three viewers, one per kind of composite object.
import '../essay/essay.css';
import './census.css';
import { mountCensusViewer } from './viewer';
import { formation, bound, passengers } from './convoys-objects';

mountCensusViewer(document.getElementById('cv-formation')!, formation);
mountCensusViewer(document.getElementById('cv-bound')!, bound);
mountCensusViewer(document.getElementById('cv-passengers')!, passengers);
